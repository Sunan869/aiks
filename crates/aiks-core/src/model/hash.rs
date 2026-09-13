use sha2::{Digest, Sha256};

use crate::model::{ContentBlock, NormalizedSession};

/// Compute a stable SHA-256 hash of a normalized session.
///
/// Only stable, content-bearing fields are included so that:
/// - Re-scanning the same session always produces the same hash
/// - Adding volatile fields (last_seen_at, scan time) does NOT change the hash
///
/// Fields excluded: last_seen_at, source_path mtime, scan timestamps
pub fn compute_session_hash(session: &NormalizedSession) -> String {
    let mut hasher = Sha256::new();

    hasher.update(session.source.as_str().as_bytes());
    hasher.update(session.external_session_id.as_bytes());

    if let Some(model) = &session.model {
        hasher.update(model.as_bytes());
    }

    for msg in &session.messages {
        hasher.update(msg.role.as_str().as_bytes());

        if let Some(ts) = &msg.created_at {
            hasher.update(ts.timestamp_millis().to_string().as_bytes());
        }

        if let Some(model) = &msg.model {
            hasher.update(model.as_bytes());
        }

        for block in &msg.blocks {
            hash_block(&mut hasher, block);
        }
    }

    hex::encode(hasher.finalize())
}

fn hash_block(hasher: &mut Sha256, block: &ContentBlock) {
    match block {
        ContentBlock::Text { text } => {
            hasher.update(b"text:");
            hasher.update(text.as_bytes());
        }
        ContentBlock::Thinking { text } => {
            hasher.update(b"thinking:");
            hasher.update(text.as_bytes());
        }
        ContentBlock::ToolCall { id, name, input } => {
            hasher.update(b"tool_call:");
            if let Some(id) = id {
                hasher.update(id.as_bytes());
            }
            hasher.update(name.as_bytes());
            hasher.update(input.to_string().as_bytes());
        }
        ContentBlock::ToolResult { id, content, is_error } => {
            hasher.update(b"tool_result:");
            if let Some(id) = id {
                hasher.update(id.as_bytes());
            }
            hasher.update(content.as_bytes());
            hasher.update(if *is_error { b"1" } else { b"0" });
        }
        ContentBlock::Image { source, .. } => {
            // Hash only the first 128 bytes of large base64 images for performance
            hasher.update(b"image:");
            let limit = source.len().min(128);
            hasher.update(&source.as_bytes()[..limit]);
        }
        ContentBlock::FileReference { path, .. } => {
            hasher.update(b"file_ref:");
            hasher.update(path.as_bytes());
        }
        ContentBlock::Unknown { raw } => {
            hasher.update(b"unknown:");
            hasher.update(raw.to_string().as_bytes());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;
    use std::collections::HashMap;

    fn make_session(msgs: Vec<NormalizedMessage>) -> NormalizedSession {
        NormalizedSession {
            source: SourceKind::ClaudeCode,
            external_session_id: "test-session-id".to_string(),
            title: Some("Test Session".to_string()),
            project_name: None,
            project_path: None,
            source_path: None,
            started_at: None,
            updated_at: None,
            model: Some("claude-opus-4-5".to_string()),
            messages: msgs,
            usage: None,
            metadata: HashMap::new(),
        }
    }

    #[test]
    fn same_session_produces_same_hash() {
        let session = make_session(vec![NormalizedMessage {
            external_id: "msg-1".to_string(),
            parent_id: None,
            role: MessageRole::User,
            created_at: None,
            model: None,
            blocks: vec![ContentBlock::Text {
                text: "Hello world".to_string(),
            }],
            usage: None,
            metadata: HashMap::new(),
        }]);
        let h1 = compute_session_hash(&session);
        let h2 = compute_session_hash(&session);
        assert_eq!(h1, h2);
    }

    #[test]
    fn different_content_produces_different_hash() {
        let s1 = make_session(vec![NormalizedMessage {
            external_id: "msg-1".to_string(),
            parent_id: None,
            role: MessageRole::User,
            created_at: None,
            model: None,
            blocks: vec![ContentBlock::Text {
                text: "Hello".to_string(),
            }],
            usage: None,
            metadata: HashMap::new(),
        }]);
        let s2 = make_session(vec![NormalizedMessage {
            external_id: "msg-1".to_string(),
            parent_id: None,
            role: MessageRole::User,
            created_at: None,
            model: None,
            blocks: vec![ContentBlock::Text {
                text: "World".to_string(),
            }],
            usage: None,
            metadata: HashMap::new(),
        }]);
        assert_ne!(compute_session_hash(&s1), compute_session_hash(&s2));
    }

    #[test]
    fn hash_is_hex_string() {
        let session = make_session(vec![]);
        let hash = compute_session_hash(&session);
        assert_eq!(hash.len(), 64); // SHA-256 = 32 bytes = 64 hex chars
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
