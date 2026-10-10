#![deny(warnings)]
#![forbid(unsafe_code)]

use crate::error::AgentError;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AdapterProfile {
    pub name: String,
    pub provider: String,
    pub allowed_cwd_roots: Option<Vec<PathBuf>>,
    pub allowed_env_keys: Vec<String>,
    pub args_template: Vec<String>,
    pub resume_template: Option<Vec<String>>,
}

impl AdapterProfile {
    pub fn new(name: impl Into<String>, provider: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            provider: provider.into(),
            ..Self::default()
        }
    }

    pub fn with_allowed_cwd_roots(mut self, roots: Vec<PathBuf>) -> Self {
        self.allowed_cwd_roots = Some(roots);
        self
    }

    pub fn with_allowed_env_keys(mut self, keys: Vec<String>) -> Self {
        self.allowed_env_keys = keys;
        self
    }

    pub fn with_args_template(mut self, template: Vec<String>) -> Self {
        self.args_template = template;
        self
    }

    pub fn with_resume_template(mut self, template: Vec<String>) -> Self {
        self.resume_template = Some(template);
        self
    }

    pub fn validate_cwd(&self, cwd: &Path) -> Result<(), AgentError> {
        if contains_parent_traversal(cwd) {
            return Err(AgentError::CwdTraversal);
        }
        match &self.allowed_cwd_roots {
            None => Ok(()),
            Some(roots) if roots.is_empty() => Ok(()),
            Some(roots) => {
                let normalized_cwd = normalize(cwd);
                let allowed = roots.iter().any(|root| {
                    normalized_cwd.starts_with(normalize(root))
                });
                if allowed {
                    Ok(())
                } else {
                    Err(AgentError::CwdNotAllowed(cwd.display().to_string()))
                }
            }
        }
    }

    pub fn check_env_key(&self, key: &str) -> Result<(), AgentError> {
        if self.allowed_env_keys.is_empty() {
            return Ok(());
        }
        if self
            .allowed_env_keys
            .iter()
            .any(|allowed| allowed == key)
        {
            Ok(())
        } else {
            Err(AgentError::EnvKeyNotAllowed(key.to_string()))
        }
    }

    pub fn validate_env_key(&self, key: &str) -> Result<(), AgentError> {
        self.check_env_key(key)
    }

    pub fn validate(&self) -> Result<(), AgentError> {
        if self.name.is_empty() {
            return Err(AgentError::InvalidProfile("name is empty".into()));
        }
        if self.provider.is_empty() {
            return Err(AgentError::InvalidProfile("provider is empty".into()));
        }
        Ok(())
    }
}

fn contains_parent_traversal(path: &Path) -> bool {
    path.components().any(|c| matches!(c, Component::ParentDir))
}

fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_cwd_under_allowed_root() {
        let profile = AdapterProfile::new("p", "claude").with_allowed_cwd_roots(vec![
            PathBuf::from("/home/user/projects"),
        ]);
        assert!(profile.validate_cwd(Path::new("/home/user/projects/app")).is_ok());
    }

    #[test]
    fn rejects_cwd_with_parent_traversal() {
        let profile = AdapterProfile::new("p", "claude");
        assert!(matches!(
            profile.validate_cwd(Path::new("/home/user/../etc")),
            Err(AgentError::CwdTraversal)
        ));
    }

    #[test]
    fn rejects_cwd_outside_allowed_root() {
        let profile = AdapterProfile::new("p", "claude").with_allowed_cwd_roots(vec![
            PathBuf::from("/home/user/projects"),
        ]);
        assert!(matches!(
            profile.validate_cwd(Path::new("/etc/passwd")),
            Err(AgentError::CwdNotAllowed(_))
        ));
    }

    #[test]
    fn empty_allowed_roots_means_any() {
        let profile = AdapterProfile::new("p", "claude").with_allowed_cwd_roots(vec![]);
        assert!(profile.validate_cwd(Path::new("/anywhere")).is_ok());
    }

    #[test]
    fn none_allowed_roots_means_any() {
        let profile = AdapterProfile::new("p", "claude");
        assert!(profile.validate_cwd(Path::new("/anywhere")).is_ok());
    }

    #[test]
    fn env_whitelist_rejects_unlisted_keys() {
        let profile = AdapterProfile::new("p", "claude").with_allowed_env_keys(vec![
            "API_TOKEN".into(),
            "PATH".into(),
        ]);
        assert!(profile.check_env_key("API_TOKEN").is_ok());
        assert!(profile.check_env_key("PATH").is_ok());
        assert!(matches!(
            profile.check_env_key("SECRET_PASSWORD"),
            Err(AgentError::EnvKeyNotAllowed(_))
        ));
    }

    #[test]
    fn env_whitelist_empty_allows_any() {
        let profile = AdapterProfile::new("p", "claude");
        assert!(profile.check_env_key("ANYTHING").is_ok());
    }
}