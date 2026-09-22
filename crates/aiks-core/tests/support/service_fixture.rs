use aiks_core::model::{
    ContentBlock, MessageRole, NormalizedMessage, NormalizedSession, SourceKind,
};
use aiks_core::service::SnapshotSubmission;
use std::collections::HashMap;

pub fn fixture_session(text: &str) -> NormalizedSession {
    NormalizedSession {
        source: SourceKind::Continue,
        external_session_id: "synthetic-1".into(),
        title: Some("Synthetic session".into()),
        project_name: None,
        project_path: None,
        source_path: None,
        started_at: None,
        updated_at: None,
        model: None,
        messages: vec![NormalizedMessage {
            external_id: "message-1".into(),
            parent_id: None,
            role: MessageRole::User,
            created_at: None,
            model: None,
            blocks: vec![ContentBlock::Text { text: text.into() }],
            usage: None,
            metadata: HashMap::new(),
        }],
        usage: None,
        metadata: HashMap::new(),
    }
}

pub fn submission(
    space: &str,
    instance: &str,
    registration: &str,
    id: &str,
    expected: u32,
    text: &str,
) -> SnapshotSubmission {
    SnapshotSubmission {
        api_version: 1,
        submission_id: id.into(),
        service_instance_id: instance.into(),
        space_id: space.into(),
        source_registration_id: registration.into(),
        expected_revision: expected,
        complete: true,
        parser_version: "continue-session-v1".into(),
        session: fixture_session(text),
    }
}
