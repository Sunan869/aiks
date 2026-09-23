//! Model secrets belong to the Service process, never to a client capability.
use std::fmt;

use serde::Deserialize;

use crate::ServiceConfig;

const AI_FIELD: &str = "model_credentials.ai_api_key_env";
const EMBEDDING_FIELD: &str = "model_credentials.embedding_api_key_env";

/// These are environment variable names, not values. No Serialize/Debug derive.
#[derive(Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ModelCredentials {
    pub ai_api_key_env: String,
    pub embedding_api_key_env: String,
}

/// Only fixed field names and safe error codes may reach diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelCredentialIssue {
    pub field: &'static str,
    pub code: &'static str,
}

impl fmt::Display for ModelCredentialIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.field, self.code)
    }
}

impl std::error::Error for ModelCredentialIssue {}

impl ServiceConfig {
    /// Resolve once after parsing and validation, before opening business state.
    /// Reloads must read the original config again, not reuse resolved secrets.
    /// Both values are checked before mutating either configuration.
    pub fn resolve_model_credentials_with(
        &mut self,
        lookup: impl Fn(&str) -> Option<String>,
    ) -> Result<(), ModelCredentialIssue> {
        let ai_name = &self.model_credentials.ai_api_key_env;
        let embedding_name = &self.model_credentials.embedding_api_key_env;
        check_reference(ai_name, self.ai.enabled, &self.ai.api_key, AI_FIELD)?;
        check_reference(
            embedding_name,
            self.embedding.enabled,
            &self.embedding.api_key,
            EMBEDDING_FIELD,
        )?;
        let ai_key = resolve_key(
            ai_name,
            self.ai.enabled,
            &self.ai.api_key,
            AI_FIELD,
            &lookup,
        )?;
        let embedding_key = resolve_key(
            embedding_name,
            self.embedding.enabled,
            &self.embedding.api_key,
            EMBEDDING_FIELD,
            &lookup,
        )?;
        self.ai.api_key = ai_key;
        self.embedding.api_key = embedding_key;
        Ok(())
    }
}

fn check_reference(
    name: &str,
    enabled: bool,
    inline: &Option<String>,
    field: &'static str,
) -> Result<(), ModelCredentialIssue> {
    if !enabled || name.is_empty() {
        return Ok(());
    }
    let valid = name.len() <= 128
        && name
            .bytes()
            .next()
            .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_');
    if !valid {
        return Err(ModelCredentialIssue {
            field,
            code: "invalid_environment_reference",
        });
    }
    if inline.is_some() {
        return Err(ModelCredentialIssue {
            field,
            code: "ambiguous_secret_source",
        });
    }
    Ok(())
}

fn resolve_key(
    name: &str,
    enabled: bool,
    inline: &Option<String>,
    field: &'static str,
    lookup: &impl Fn(&str) -> Option<String>,
) -> Result<Option<String>, ModelCredentialIssue> {
    if !enabled || name.is_empty() {
        return Ok(inline.clone());
    }
    let value = lookup(name).ok_or(ModelCredentialIssue {
        field,
        code: "secret_missing",
    })?;
    if value.is_empty() || value.len() > 8192 || !value.bytes().all(|b| (0x21..=0x7e).contains(&b)) {
        return Err(ModelCredentialIssue {
            field,
            code: "secret_invalid",
        });
    }
    Ok(Some(value))
}
