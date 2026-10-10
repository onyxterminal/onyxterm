#![deny(warnings)]
#![forbid(unsafe_code)]

use crate::adapter::{Adapter, CommandSpec};
use crate::error::AgentError;
use crate::profile::AdapterProfile;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub const PLACEHOLDER_CWD: &str = "cwd";
pub const PLACEHOLDER_PROMPT: &str = "prompt";

#[derive(Default)]
pub struct AdapterRegistry {
    adapters: HashMap<String, Box<dyn Adapter>>,
}

impl AdapterRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_defaults() -> Self {
        let mut registry = Self::new();
        registry.register(Box::new(GenericCliAdapter::new(
            "claude",
            "Claude Code",
            Some(PathBuf::from("claude")),
        )));
        registry.register(Box::new(GenericCliAdapter::new(
            "codex",
            "Codex CLI",
            Some(PathBuf::from("codex")),
        )));
        registry
    }

    pub fn register(&mut self, adapter: Box<dyn Adapter>) {
        self.adapters.insert(adapter.id().to_string(), adapter);
    }

    pub fn get(&self, id: &str) -> Option<&dyn Adapter> {
        self.adapters.get(id).map(|a| a.as_ref())
    }

    pub fn list(&self) -> Vec<(&str, &str)> {
        let mut entries: Vec<(&str, &str)> = self
            .adapters
            .values()
            .map(|a| (a.id(), a.display_name()))
            .collect();
        entries.sort_by(|a, b| a.0.cmp(b.0));
        entries
    }

    pub fn ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.adapters.keys().cloned().collect();
        ids.sort();
        ids
    }
}

pub struct GenericCliAdapter {
    id: String,
    display_name: String,
    executable: Option<PathBuf>,
}

impl GenericCliAdapter {
    pub fn new(
        id: impl Into<String>,
        display_name: impl Into<String>,
        executable: Option<PathBuf>,
    ) -> Self {
        Self {
            id: id.into(),
            display_name: display_name.into(),
            executable,
        }
    }

    pub fn substitute_args(template: &[String], cwd: &Path) -> Result<Vec<String>, AgentError> {
        let cwd_str = cwd.display().to_string();
        template
            .iter()
            .map(|arg| substitute_placeholders(arg, &cwd_str))
            .collect()
    }
}

impl Adapter for GenericCliAdapter {
    fn id(&self) -> &str {
        &self.id
    }

    fn display_name(&self) -> &str {
        &self.display_name
    }

    fn resolve_executable(&self) -> Option<PathBuf> {
        self.executable.clone()
    }

    fn build_command(
        &self,
        profile: &AdapterProfile,
        cwd: PathBuf,
    ) -> Result<CommandSpec, AgentError> {
        let executable = self
            .executable
            .clone()
            .ok_or_else(|| AgentError::NoExecutable(self.id.clone()))?;
        let args = Self::substitute_args(&profile.args_template, &cwd)?;
        let mut env = std::collections::BTreeMap::new();
        for key in &profile.allowed_env_keys {
            env.insert(key.clone(), String::new());
        }
        Ok(CommandSpec::new(executable, args, env, cwd))
    }
}

fn substitute_placeholders(arg: &str, cwd: &str) -> Result<String, AgentError> {
    let mut out = String::with_capacity(arg.len());
    let mut rest = arg;
    while let Some(open_idx) = rest.find('{') {
        out.push_str(&rest[..open_idx]);
        rest = &rest[open_idx + 1..];
        let close_idx = rest.find('}').ok_or_else(|| {
            AgentError::TemplatePlaceholder(format!("unclosed brace in '{}'", arg))
        })?;
        let placeholder = &rest[..close_idx];
        match placeholder {
            PLACEHOLDER_CWD => out.push_str(cwd),
            PLACEHOLDER_PROMPT => {
                return Err(AgentError::TemplatePlaceholder(PLACEHOLDER_PROMPT.into()));
            }
            _ => {
                return Err(AgentError::TemplatePlaceholder(placeholder.into()));
            }
        }
        rest = &rest[close_idx + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_contains_claude_and_codex() {
        let registry = AdapterRegistry::with_defaults();
        assert!(registry.get("claude").is_some());
        assert!(registry.get("codex").is_some());
        let list = registry.list();
        let ids: Vec<&str> = list.iter().map(|(id, _)| *id).collect();
        assert!(ids.contains(&"claude"));
        assert!(ids.contains(&"codex"));
    }

    #[test]
    fn generic_adapter_builds_argv_no_shell() {
        let adapter = GenericCliAdapter::new(
            "claude",
            "Claude",
            Some(PathBuf::from("/usr/local/bin/claude")),
        );
        let profile = AdapterProfile::new("p", "claude").with_args_template(vec![
            "--cwd".into(),
            "{cwd}".into(),
            "--resume".into(),
        ]);
        let spec = adapter
            .build_command(&profile, PathBuf::from("/home/user/proj"))
            .unwrap();
        assert_eq!(spec.executable, PathBuf::from("/usr/local/bin/claude"));
        assert_eq!(spec.args.len(), 3);
        assert_eq!(spec.args[0], "--cwd");
        assert_eq!(spec.args[1], "/home/user/proj");
        assert_eq!(spec.args[2], "--resume");
        assert_eq!(spec.cwd, PathBuf::from("/home/user/proj"));
    }

    #[test]
    fn generic_adapter_substitutes_placeholders() {
        let args = GenericCliAdapter::substitute_args(
            &["{cwd}".into(), "--cwd={cwd}".into(), "literal".into()],
            Path::new("/tmp/work"),
        )
        .unwrap();
        assert_eq!(args, vec!["/tmp/work", "--cwd=/tmp/work", "literal"]);
    }

    #[test]
    fn generic_adapter_rejects_prompt_placeholder() {
        let err = GenericCliAdapter::substitute_args(
            &["{prompt}".into()],
            Path::new("/tmp"),
        )
        .unwrap_err();
        assert!(matches!(err, AgentError::TemplatePlaceholder(_)));
    }

    #[test]
    fn generic_adapter_no_executable_errors() {
        let adapter = GenericCliAdapter::new("ghost", "Ghost", None);
        let profile = AdapterProfile::new("p", "ghost");
        let err = adapter
            .build_command(&profile, PathBuf::from("/tmp"))
            .unwrap_err();
        assert!(matches!(err, AgentError::NoExecutable(_)));
    }

    #[test]
    fn classify_exit_default_rules() {
        let adapter = GenericCliAdapter::new("claude", "Claude", Some(PathBuf::from("claude")));
        assert_eq!(adapter.classify_exit(Some(0)), domain::AgentState::Exited);
        assert_eq!(adapter.classify_exit(Some(1)), domain::AgentState::Failed);
        assert_eq!(adapter.classify_exit(None), domain::AgentState::Failed);
    }
}