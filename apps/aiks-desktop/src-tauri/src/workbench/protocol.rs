#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_bridge_protocol_v1() {
        assert!(validate_protocol_version(1).is_ok());
        assert!(validate_protocol_version(2).is_err());
        assert!(validate_protocol_version(0).is_err());
    }

    #[test]
    fn accepts_only_loopback_http_origins() {
        assert!(validate_loopback_origin("http://127.0.0.1:6806").is_ok());
        assert!(validate_loopback_origin("http://localhost:6806").is_ok());
        assert!(validate_loopback_origin("http://[::1]:6806").is_ok());
        assert!(validate_loopback_origin("https://example.com").is_err());
        assert!(validate_loopback_origin("http://192.0.2.1:6806").is_err());
    }

    #[test]
    fn rejects_missing_document_and_block_ids() {
        assert!(validate_identifier("doc_id", "20260916000100-abcdefg").is_ok());
        assert!(validate_identifier("doc_id", "").is_err());
        assert!(validate_identifier("block_id", "   ").is_err());
    }

    #[test]
    fn workspace_mode_parser_rejects_unknown_modes() {
        assert_eq!(WorkspaceMode::parse("knowledge").unwrap(), WorkspaceMode::Knowledge);
        assert_eq!(WorkspaceMode::parse("session").unwrap(), WorkspaceMode::Session);
        assert!(WorkspaceMode::parse("admin").is_err());
        assert!(WorkspaceMode::parse("").is_err());
    }
}
