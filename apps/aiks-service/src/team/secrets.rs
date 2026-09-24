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
    let value = match source {
        SecretSource::Environment(name) => {
            if !valid_environment_name(name) {
                return Err(issue("invalid_environment_reference"));
            }
            lookup(name).ok_or_else(|| issue("secret_missing"))?
        }
        SecretSource::File(path) => secure_file(path)?,
    };
    validate_value(value, issue)
}

fn validate_value(
    value: String,
    issue: impl Fn(&'static str) -> ConfigIssue,
) -> Result<SecretValue, ConfigIssue> {
    if value.is_empty()
        || value.len() > 8192
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        return Err(issue("secret_invalid"));
    }
    Ok(SecretValue(value))
}

#[cfg(unix)]
fn secure_file(path: &std::path::Path) -> Result<String, ConfigIssue> {
    use std::{
        io::Read,
        os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    };
    let issue = |code| ConfigIssue { field: FIELD, code };
    let before = std::fs::symlink_metadata(path).map_err(|_| issue("secret_missing"))?;
    if !before.file_type().is_file() || before.file_type().is_symlink() {
        return Err(issue("secret_file_unsafe"));
    }
    if before.permissions().mode() & 0o077 != 0 {
        return Err(issue("secret_file_permissions"));
    }
    #[cfg(target_os = "linux")]
    {
        let owner = std::fs::metadata("/proc/self").map_err(|_| issue("secret_file_unsafe"))?;
        if before.uid() != owner.uid() {
            return Err(issue("secret_file_owner"));
        }
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    let mut file = options.open(path).map_err(|_| issue("secret_file_unsafe"))?;
    let after = file.metadata().map_err(|_| issue("secret_file_unsafe"))?;
    if !after.is_file()
        || after.dev() != before.dev()
        || after.ino() != before.ino()
        || after.len() > 8192
    {
        return Err(issue("secret_file_unsafe"));
    }
    let mut value = String::new();
    file.take(8193)
        .read_to_string(&mut value)
        .map_err(|_| issue("secret_invalid"))?;
    if value.len() > 8192 {
        return Err(issue("secret_invalid"));
    }
    Ok(value)
}

#[cfg(not(unix))]
fn secure_file(_: &std::path::Path) -> Result<String, ConfigIssue> {
    Err(ConfigIssue {
        field: FIELD,
        code: "secret_file_not_supported",
    })
}
