use std::collections::BTreeMap;
use std::io::{self, Write};

use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::model::ContentBlock;

use super::contracts::{
    ServiceError, SnapshotSubmission, ValidatedSnapshot, API_VERSION, MAX_BLOCKS_PER_MESSAGE,
    MAX_BODY_BYTES, MAX_JSON_DEPTH, MAX_JSON_NODES, MAX_MESSAGES,
};

/// Pure validation: source paths and URLs remain data, never IO instructions.
pub fn validate_submission(
    request: &SnapshotSubmission,
) -> Result<ValidatedSnapshot, ServiceError> {
    if request.api_version != API_VERSION {
        return Err(ServiceError::UnsupportedVersion);
    }
    if !request.complete {
        return Err(ServiceError::IncompleteSnapshot);
    }
    for id in [
        &request.submission_id,
        &request.service_instance_id,
        &request.space_id,
        &request.source_registration_id,
        &request.parser_version,
        &request.session.external_session_id,
    ] {
        validate_identifier(id)?;
    }
    if request.session.messages.len() > MAX_MESSAGES {
        return Err(ServiceError::TooLarge);
    }

    // Check dynamic JSON before Serde recursively serializes it. A bounded
    // writer then checks the entire envelope, including UTF-8 and escaping.
    let mut remaining_nodes = MAX_JSON_NODES;
    for value in request.session.metadata.values() {
        validate_json(value, 1, &mut remaining_nodes)?;
    }
    for message in &request.session.messages {
        if message.blocks.len() > MAX_BLOCKS_PER_MESSAGE {
            return Err(ServiceError::TooLarge);
        }
        for value in message.metadata.values() {
            validate_json(value, 1, &mut remaining_nodes)?;
        }
        for block in &message.blocks {
            match block {
                ContentBlock::ToolCall { input, .. } => {
                    validate_json(input, 1, &mut remaining_nodes)?;
                }
                ContentBlock::Unknown { raw } => {
                    validate_json(raw, 1, &mut remaining_nodes)?;
                }
                _ => {}
            }
        }
    }

    let encoded = bounded_json(request)?;
    let mut envelope: Value =
        serde_json::from_slice(&encoded).map_err(|_| ServiceError::InvalidInput)?;
    envelope
        .as_object_mut()
        .ok_or(ServiceError::InvalidInput)?
        .remove("submission_id");
    let request_hash = canonical_hash(&envelope)?;
    let session = envelope.get("session").ok_or(ServiceError::InvalidInput)?;
    let canonical_json = canonical_bytes(session)?;
    let content_hash = canonical_hash(&serde_json::json!({
        "parser_version": request.parser_version,
        "session": session,
    }))?;
    Ok(ValidatedSnapshot {
        submission: request.clone(),
        canonical_json,
        request_hash,
        content_hash,
    })
}

pub(super) fn validate_identifier(value: &str) -> Result<(), ServiceError> {
    if value.is_empty()
        || value.len() > 512
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        return Err(ServiceError::InvalidInput);
    }
    Ok(())
}

fn validate_json(value: &Value, depth: usize, remaining: &mut usize) -> Result<(), ServiceError> {
    if depth > MAX_JSON_DEPTH || *remaining == 0 {
        return Err(ServiceError::TooLarge);
    }
    *remaining -= 1;
    match value {
        Value::Array(items) => {
            for item in items {
                validate_json(item, depth + 1, remaining)?;
            }
        }
        Value::Object(items) => {
            for item in items.values() {
                validate_json(item, depth + 1, remaining)?;
            }
        }
        _ => {}
    }
    Ok(())
}

struct BoundedWriter(Vec<u8>);

impl Write for BoundedWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > MAX_BODY_BYTES.saturating_sub(self.0.len()) {
            return Err(io::Error::other("snapshot budget exceeded"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn bounded_json(value: &impl Serialize) -> Result<Vec<u8>, ServiceError> {
    let mut writer = BoundedWriter(Vec::new());
    serde_json::to_writer(&mut writer, value).map_err(|error| {
        if error.is_io() {
            ServiceError::TooLarge
        } else {
            ServiceError::InvalidInput
        }
    })?;
    Ok(writer.0)
}

// Explicit sorting remains deterministic even if another crate enables
// serde_json's preserve_order feature. Array ordering is deliberately retained.
fn sorted_json(value: &Value) -> Value {
    match value {
        Value::Object(object) => {
            let ordered: BTreeMap<_, _> = object
                .iter()
                .map(|(key, item)| (key.clone(), sorted_json(item)))
                .collect();
            Value::Object(ordered.into_iter().collect())
        }
        Value::Array(items) => Value::Array(items.iter().map(sorted_json).collect()),
        _ => value.clone(),
    }
}

fn canonical_bytes(value: &Value) -> Result<Vec<u8>, ServiceError> {
    bounded_json(&sorted_json(value))
}

fn canonical_hash(value: &Value) -> Result<String, ServiceError> {
    Ok(hex::encode(Sha256::digest(canonical_bytes(value)?)))
}
