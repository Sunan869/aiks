use aiks_core::model::ContentBlock;
use aiks_core::service::{validate_submission, SnapshotSubmission};
use serde_json::json;

#[path = "support/service_fixture.rs"]
mod fixture;

fn input() -> SnapshotSubmission {
    fixture::submission(
        "space",
        "instance",
        "registration",
        "upload-1",
        0,
        "秘密正文",
    )
}

#[test]
fn valid_snapshot_preserves_unknown_blocks_and_does_not_print_content() {
    let mut request = input();
    request.session.messages[0]
        .blocks
        .push(ContentBlock::Unknown {
            raw: json!({"future": "sensitive-unknown"}),
        });
    request.session.source_path = Some("/never/read/this/file.jsonl".into());
    request
        .session
        .metadata
        .insert("url".into(), json!("http://127.0.0.1:1/private"));
    let accepted = validate_submission(&request).unwrap();
    let stored: serde_json::Value = serde_json::from_slice(&accepted.canonical_json).unwrap();
    assert_eq!(stored["messages"][0]["blocks"].as_array().unwrap().len(), 2);
    let debug = format!("{accepted:?}");
    assert!(!debug.contains("秘密正文"));
    assert!(!debug.contains("sensitive-unknown"));
    assert!(!debug.contains("/never/read"));
}

#[test]
fn incomplete_snapshot_has_a_stable_error() {
    let mut request = input();
    request.complete = false;
    assert_eq!(
        validate_submission(&request).unwrap_err().code(),
        "incomplete_snapshot"
    );
}

#[test]
fn unsupported_protocol_and_empty_identifiers_are_rejected() {
    let mut request = input();
    request.api_version = 2;
    assert_eq!(
        validate_submission(&request).unwrap_err().code(),
        "unsupported_version"
    );
    request.api_version = 1;
    for field in [
        "submission_id",
        "service_instance_id",
        "space_id",
        "source_registration_id",
        "parser_version",
    ] {
        let mut value = serde_json::to_value(&request).unwrap();
        value[field] = json!(" ");
        let invalid: SnapshotSubmission = serde_json::from_value(value).unwrap();
        assert_eq!(
            validate_submission(&invalid).unwrap_err().code(),
            "invalid_input"
        );
    }
    request.session.external_session_id.clear();
    assert!(validate_submission(&request).is_err());
}

#[test]
fn clients_cannot_smuggle_owner_or_role_fields() {
    for field in ["owner_id", "principal_id", "role"] {
        let mut value = serde_json::to_value(input()).unwrap();
        value[field] = json!("administrator");
        assert!(serde_json::from_value::<SnapshotSubmission>(value).is_err());
    }
}

#[test]
fn fingerprints_are_canonical_and_submission_id_is_not_payload_identity() {
    let mut first = input();
    first
        .session
        .metadata
        .insert("z".into(), json!({"b": 2, "a": 1}));
    first.session.metadata.insert("a".into(), json!(1));
    let mut second = first.clone();
    second.submission_id = "upload-2".into();
    second.session.metadata.clear();
    second.session.metadata.insert("a".into(), json!(1));
    second
        .session
        .metadata
        .insert("z".into(), json!({"a": 1, "b": 2}));
    let a = validate_submission(&first).unwrap();
    let b = validate_submission(&second).unwrap();
    assert_eq!(a.request_hash, b.request_hash);
    assert_eq!(a.content_hash, b.content_hash);
    assert_eq!(a.canonical_json, b.canonical_json);
    second.expected_revision = 1;
    let c = validate_submission(&second).unwrap();
    assert_ne!(a.request_hash, c.request_hash);
    assert_eq!(a.content_hash, c.content_hash);
}

#[test]
fn title_parser_and_message_order_are_revision_content() {
    let original = input();
    let hash = validate_submission(&original).unwrap().content_hash;
    let mut title = original.clone();
    title.session.title = Some("Renamed".into());
    assert_ne!(hash, validate_submission(&title).unwrap().content_hash);
    let mut parser = original.clone();
    parser.parser_version = "continue-session-v2".into();
    assert_ne!(hash, validate_submission(&parser).unwrap().content_hash);
    let mut order = original;
    let mut message = order.session.messages[0].clone();
    message.external_id = "message-2".into();
    order.session.messages.push(message);
    let before = validate_submission(&order).unwrap().content_hash;
    order.session.messages.reverse();
    assert_ne!(before, validate_submission(&order).unwrap().content_hash);
}

#[test]
fn message_and_block_budgets_are_enforced_without_truncating() {
    let mut messages = input();
    messages.session.messages = vec![messages.session.messages[0].clone(); 20_001];
    assert_eq!(
        validate_submission(&messages).unwrap_err().code(),
        "too_large"
    );
    let mut blocks = input();
    blocks.session.messages[0].blocks = vec![ContentBlock::Text { text: "x".into() }; 257];
    assert_eq!(
        validate_submission(&blocks).unwrap_err().code(),
        "too_large"
    );
}

#[test]
fn serialized_byte_budget_and_nested_unknown_data_are_bounded() {
    let mut oversized = input();
    oversized.session.messages[0].blocks = vec![ContentBlock::Text {
        text: "x".repeat(16 * 1024 * 1024 + 1),
    }];
    assert_eq!(
        validate_submission(&oversized).unwrap_err().code(),
        "too_large"
    );
    let mut deep = input();
    let mut value = json!("leaf");
    for _ in 0..80 {
        value = json!({"nested": value});
    }
    deep.session.metadata.insert("tree".into(), value);
    assert_eq!(validate_submission(&deep).unwrap_err().code(), "too_large");
}
