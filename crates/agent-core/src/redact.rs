#![deny(warnings)]
#![forbid(unsafe_code)]

use std::collections::BTreeMap;

const REDACTED: &str = "***";

pub fn redact_env(env: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    env.iter()
        .map(|(k, v)| {
            let value = if is_sensitive(k) {
                REDACTED.to_string()
            } else {
                v.clone()
            };
            (k.clone(), value)
        })
        .collect()
}

pub fn is_sensitive(key: &str) -> bool {
    let lower = key.to_lowercase();
    lower.contains("token")
        || lower.contains("key")
        || lower.contains("secret")
        || lower.contains("password")
        || lower.contains("auth")
        || lower.contains("credential")
}

pub fn mask_value(value: &str) -> String {
    if value.is_empty() {
        String::new()
    } else {
        REDACTED.to_string()
    }
}

#[allow(dead_code)]
pub fn redacted() -> &'static str {
    REDACTED
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_token_key_secret_password_auth_credential() {
        let mut env = BTreeMap::new();
        env.insert("API_TOKEN".into(), "abc".into());
        env.insert("PRIVATE_KEY".into(), "xyz".into());
        env.insert("CLIENT_SECRET".into(), "hidden".into());
        env.insert("DB_PASSWORD".into(), "pw".into());
        env.insert("OAUTH".into(), "token".into());
        env.insert("CREDENTIAL_PATH".into(), "/creds".into());
        env.insert("PATH".into(), "/usr/bin".into());
        env.insert("HOME".into(), "/home/user".into());

        let redacted = redact_env(&env);

        assert_eq!(redacted.get("API_TOKEN"), Some(&"***".to_string()));
        assert_eq!(redacted.get("PRIVATE_KEY"), Some(&"***".to_string()));
        assert_eq!(redacted.get("CLIENT_SECRET"), Some(&"***".to_string()));
        assert_eq!(redacted.get("DB_PASSWORD"), Some(&"***".to_string()));
        assert_eq!(redacted.get("OAUTH"), Some(&"***".to_string()));
        assert_eq!(redacted.get("CREDENTIAL_PATH"), Some(&"***".to_string()));
        assert_eq!(redacted.get("PATH"), Some(&"/usr/bin".to_string()));
        assert_eq!(redacted.get("HOME"), Some(&"/home/user".to_string()));
    }

    #[test]
    fn mask_value_handles_empty() {
        assert_eq!(mask_value(""), "");
        assert_eq!(mask_value("x"), "***");
    }
}