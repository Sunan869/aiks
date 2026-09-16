#[cfg(test)]
mod tests {
    use super::*;
    use crate::workbench::protocol::{WorkbenchAction, WorkspaceMode};

    #[test]
    fn bridge_envelope_contains_protocol_request_nonce_action_and_payload() {
        let controller = WorkbenchController::new();
        let envelope = BridgeEnvelope::for_action(
            &controller,
            WorkbenchAction::OpenDocument {
                doc_id: "20260916000100-abcdefg".to_string(),
            },
        )
        .unwrap();
        let json = serde_json::to_value(envelope).unwrap();

        assert_eq!(json["version"], 1);
        assert!(json["requestId"].as_str().is_some_and(|value| !value.is_empty()));
        assert_eq!(json["nonce"], controller.nonce());
        assert_eq!(json["action"], "openDocument");
        assert_eq!(json["payload"]["docId"], "20260916000100-abcdefg");
    }

    #[test]
    fn bridge_envelope_rejects_invalid_document_and_block_ids() {
        let controller = WorkbenchController::new();
        assert!(BridgeEnvelope::for_action(
            &controller,
            WorkbenchAction::OpenDocument {
                doc_id: " ".to_string(),
            },
        )
        .is_err());
        assert!(BridgeEnvelope::for_action(
            &controller,
            WorkbenchAction::OpenBlock {
                doc_id: "doc".to_string(),
                block_id: "".to_string(),
            },
        )
        .is_err());
    }

    #[test]
    fn workspace_mode_is_serialized_inside_payload() {
        let controller = WorkbenchController::new();
        let envelope = BridgeEnvelope::for_action(
            &controller,
            WorkbenchAction::SetWorkspaceMode {
                mode: WorkspaceMode::Session,
            },
        )
        .unwrap();
        let json = serde_json::to_value(envelope).unwrap();

        assert_eq!(json["action"], "setWorkspaceMode");
        assert_eq!(json["payload"]["mode"], "session");
    }
}
