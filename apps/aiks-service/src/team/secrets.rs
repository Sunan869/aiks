//! Server-only secret resolution. No value is serializable or printable.
use std::fmt;

pub use super::config::SecretSource;
use super::config::{valid_environment_name, ConfigIssue};

const FIELD: &str = "team.dingtalk.client_secret";

pub struct SecretValue(String);
impl SecretValue {
    pub fn expose(&self) -> &str {
        &self.0
    }
}
impl fmt::Debug for SecretValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED]")
    }
}

pub fn resolve_secret(source: &SecretSource) -> Result<SecretValue, ConfigIssue> {
    resolve_secret_with(source, |name| std::env::var(name).ok())
}

pub fn resolve_secret_with(
    source: &SecretSource,
    lookup: impl Fn(&str) -> Option<String>,
) -> Result<SecretValue, ConfigIssue> {
    let issue = |code| ConfigIssue { field: FIELD, code };
    let name = match source {
        SecretSource::Environment(name) => name,
        // Fail closed until OS-specific descriptor/ACL validation is available.
        // Never implement the promised file policy as a bare read_to_string.
        SecretSource::File(_) => return Err(issue("secret_file_not_supported")),
    };
    if !valid_environment_name(name) {
        return Err(issue("invalid_environment_reference"));
    }
    let value = lookup(name).ok_or_else(|| issue("secret_missing"))?;
    if value.is_empty()
        || value.len() > 8192
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        return Err(issue("secret_invalid"));
    }
    Ok(SecretValue(value))
}
