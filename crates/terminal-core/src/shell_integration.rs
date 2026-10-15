use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

const ZSH_PROGRAM: &str = "zsh";
const ZSH_DIR: &str = "zsh";
const ZSH_ENV_FILE: &str = ".zshenv";
const ZSH_SCRIPT_FILE: &str = "onyx-integration.zsh";
const ZSH_ENV: &str = include_str!("../../../assets/shell-integration/zsh/zshenv");
const ZSH_SCRIPT: &str = include_str!("../../../assets/shell-integration/zsh/onyx-integration.zsh");
const ZDOTDIR_VAR: &str = "ZDOTDIR";
const USER_ZDOTDIR_VAR: &str = "ONYX_USER_ZDOTDIR";
const SCRIPT_VAR: &str = "ONYX_SHELL_INTEGRATION";

#[derive(Debug, Clone)]
pub struct ShellIntegration {
    zsh_dir: PathBuf,
}

impl ShellIntegration {
    pub fn install(root: &Path) -> io::Result<Self> {
        let zsh_dir = root.join(ZSH_DIR);
        std::fs::create_dir_all(&zsh_dir)?;
        write_if_changed(&zsh_dir.join(ZSH_ENV_FILE), ZSH_ENV)?;
        write_if_changed(&zsh_dir.join(ZSH_SCRIPT_FILE), ZSH_SCRIPT)?;
        Ok(Self { zsh_dir })
    }

    pub fn env_for(&self, program: &Path, user_zdotdir: Option<OsString>) -> Vec<(String, String)> {
        if program.file_name().and_then(|name| name.to_str()) != Some(ZSH_PROGRAM) {
            return Vec::new();
        }
        let mut env = vec![
            (ZDOTDIR_VAR.to_string(), self.zsh_dir.to_string_lossy().into_owned()),
            (SCRIPT_VAR.to_string(), self.zsh_dir.join(ZSH_SCRIPT_FILE).to_string_lossy().into_owned()),
        ];
        if let Some(original) = user_zdotdir {
            env.push((USER_ZDOTDIR_VAR.to_string(), original.to_string_lossy().into_owned()));
        }
        env
    }
}

fn write_if_changed(path: &Path, contents: &str) -> io::Result<()> {
    if std::fs::read_to_string(path).is_ok_and(|current| current == contents) {
        return Ok(());
    }
    std::fs::write(path, contents)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("onyx-integration-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        root
    }

    #[test]
    fn install_writes_the_zsh_shim_files() {
        let root = temp_root("install");
        let integration = ShellIntegration::install(&root).unwrap();
        assert_eq!(std::fs::read_to_string(root.join("zsh/.zshenv")).unwrap(), ZSH_ENV);
        assert_eq!(std::fs::read_to_string(root.join("zsh/onyx-integration.zsh")).unwrap(), ZSH_SCRIPT);
        assert_eq!(integration.zsh_dir, root.join("zsh"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn zsh_gets_a_zdotdir_shim_and_keeps_the_user_zdotdir() {
        let integration = ShellIntegration { zsh_dir: PathBuf::from("/shim/zsh") };
        let env = integration.env_for(Path::new("/bin/zsh"), Some("/home/me/.config/zsh".into()));
        assert!(env.contains(&("ZDOTDIR".into(), "/shim/zsh".into())));
        assert!(env.contains(&("ONYX_SHELL_INTEGRATION".into(), "/shim/zsh/onyx-integration.zsh".into())));
        assert!(env.contains(&("ONYX_USER_ZDOTDIR".into(), "/home/me/.config/zsh".into())));
        let without_user = integration.env_for(Path::new("/bin/zsh"), None);
        assert!(without_user.iter().all(|(key, _)| key != "ONYX_USER_ZDOTDIR"));
    }

    #[test]
    fn other_shells_are_left_untouched() {
        let integration = ShellIntegration { zsh_dir: PathBuf::from("/shim/zsh") };
        assert!(integration.env_for(Path::new("/bin/bash"), None).is_empty());
        assert!(integration.env_for(Path::new("/usr/bin/ssh"), None).is_empty());
    }
}
