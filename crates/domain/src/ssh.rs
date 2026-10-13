use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SshHost {
    pub alias: String,
    pub hostname: Option<String>,
    pub user: Option<String>,
    pub port: Option<u16>,
    pub identity_file: Option<PathBuf>,
    pub options: Vec<String>,
}

impl fmt::Display for SshHost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Some(host) = self.hostname.as_ref() else {
            return f.write_str(&self.alias);
        };
        write!(f, "{} -> ", self.alias)?;
        if let Some(user) = self.user.as_ref() {
            write!(f, "{user}@")?;
        }
        write!(f, "{host}")?;
        if let Some(port) = self.port {
            write!(f, ":{port}")?;
        }
        Ok(())
    }
}

pub fn parse_ssh_config(input: &str) -> Vec<SshHost> {
    let mut hosts = Vec::new();
    let mut current: Option<SshHost> = None;
    for raw in input.lines() {
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        let Some((key, value)) = split_directive(line) else {
            continue;
        };
        let key_lc = key.to_ascii_lowercase();
        let value = value.trim();
        match key_lc.as_str() {
            "host" => {
                if let Some(host) = current.take() {
                    hosts.push(host);
                }
                let alias = value.split_whitespace().find(|p| !is_pattern(p));
                if let Some(alias) = alias {
                    current = Some(SshHost {
                        alias: alias.to_string(),
                        hostname: None,
                        user: None,
                        port: None,
                        identity_file: None,
                        options: Vec::new(),
                    });
                } else {
                    current = None;
                }
            }
            "match" => {
                current = None;
            }
            "hostname" => assign(&mut current, |host| host.hostname = Some(value.to_string())),
            "user" => assign(&mut current, |host| host.user = Some(value.to_string())),
            "port" => assign(&mut current, |host| {
                if let Ok(port) = value.parse::<u16>() {
                    host.port = Some(port);
                }
            }),
            "identityfile" => assign(&mut current, |host| host.identity_file = Some(PathBuf::from(value))),
            _ => assign(&mut current, |host| host.options.push(format!("{key}={value}"))),
        }
    }
    if let Some(host) = current.take() {
        hosts.push(host);
    }
    hosts
}

pub fn read_ssh_hosts(path: &Path) -> Vec<SshHost> {
    let Ok(content) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let mut hosts = parse_ssh_config(&content);
    for pattern in collect_includes(&content) {
        let Some(resolved) = resolve_include(&pattern, parent) else {
            continue;
        };
        let Ok(extra) = std::fs::read_to_string(&resolved) else {
            continue;
        };
        hosts.extend(parse_ssh_config(&extra));
    }
    hosts
}

fn strip_comment(line: &str) -> &str {
    match line.find('#') {
        Some(idx) => &line[..idx],
        None => line,
    }
}

fn split_directive(line: &str) -> Option<(&str, &str)> {
    let trimmed = line.trim_start();
    let (key, rest) = trimmed.split_once(|c: char| c.is_whitespace() || c == '=')?;
    Some((key, rest.trim_start()))
}

fn is_pattern(token: &str) -> bool {
    token.contains('*') || token.contains('?') || token.contains('!')
}

fn assign<F: FnOnce(&mut SshHost)>(host: &mut Option<SshHost>, f: F) {
    if let Some(host) = host.as_mut() {
        f(host);
    }
}

fn collect_includes(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    for raw in input.lines() {
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        let Some((key, value)) = split_directive(line) else {
            continue;
        };
        if key.eq_ignore_ascii_case("include") {
            for token in value.split_whitespace() {
                out.push(token.to_string());
            }
        }
    }
    out
}

fn resolve_include(pattern: &str, base: &Path) -> Option<PathBuf> {
    if pattern == "~" || pattern.starts_with("~/") || pattern.starts_with("~\\") {
        let home = std::env::var_os("HOME").map(PathBuf::from)?;
        let suffix = &pattern[2..];
        return Some(home.join(suffix));
    }
    let candidate = PathBuf::from(pattern);
    if candidate.is_absolute() {
        Some(candidate)
    } else {
        Some(base.join(candidate))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_host_block() {
        let input = "\
Host production
    HostName prod.example.com
    User deploy
    Port 2222
    IdentityFile ~/.ssh/id_prod
";
        let hosts = parse_ssh_config(input);
        assert_eq!(hosts.len(), 1);
        let host = &hosts[0];
        assert_eq!(host.alias, "production");
        assert_eq!(host.hostname.as_deref(), Some("prod.example.com"));
        assert_eq!(host.user.as_deref(), Some("deploy"));
        assert_eq!(host.port, Some(2222));
        assert_eq!(host.identity_file.as_deref(), Some(Path::new("~/.ssh/id_prod")));
        assert!(host.options.is_empty());
        assert_eq!(host.to_string(), "production -> deploy@prod.example.com:2222");
    }

    #[test]
    fn skips_wildcard_and_match_blocks() {
        let input = "\
Host *
    User default

Match host *.internal
    User admin

Host staging
    HostName stage.example.com
";
        let hosts = parse_ssh_config(input);
        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].alias, "staging");
        assert_eq!(hosts[0].user, None);
        assert_eq!(hosts[0].hostname.as_deref(), Some("stage.example.com"));
    }

    #[test]
    fn ignores_comments_and_blank_lines() {
        let input = "\
# top-level comment

Host web  # inline comment
    HostName web.example.com # trailing
    # blank directive above
    User ops
";
        let hosts = parse_ssh_config(input);
        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].alias, "web");
        assert_eq!(hosts[0].hostname.as_deref(), Some("web.example.com"));
        assert_eq!(hosts[0].user.as_deref(), Some("ops"));
    }

    #[test]
    fn includes_external_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let included = dir.path().join("extra");
        std::fs::write(&included, "Host extra\n    HostName extra.example.com\n").expect("write extra");
        let main = dir.path().join("config");
        std::fs::write(
            &main,
            format!("Include {}\nHost main\n    HostName main.example.com\n", included.display()),
        )
        .expect("write main");
        let mut hosts = read_ssh_hosts(&main);
        hosts.sort_by(|a, b| a.alias.cmp(&b.alias));
        assert_eq!(hosts.len(), 2);
        assert_eq!(hosts[0].alias, "extra");
        assert_eq!(hosts[1].alias, "main");
    }

    #[test]
    fn returns_empty_when_config_missing() {
        let hosts = read_ssh_hosts(Path::new("/nonexistent/.ssh/config"));
        assert!(hosts.is_empty());
    }
}
