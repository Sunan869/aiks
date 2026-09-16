#[cfg(test)]
mod tests {
    use super::*;
    use crate::workbench::protocol::WorkspaceMode;

    #[test]
    fn controller_starts_unavailable_and_not_ready() {
        let controller = WorkbenchController::new();
        let status = controller.status();
        assert!(!status.available);
        assert!(!status.ready);
        assert_eq!(status.mode, WorkspaceMode::Knowledge);
        assert!(status.origin.is_none());
        assert!(!controller.nonce().is_empty());
    }

    #[test]
    fn controller_accepts_only_loopback_origin() {
        let controller = WorkbenchController::new();
        controller.set_origin("http://127.0.0.1:6806").unwrap();
        assert_eq!(
            controller.status().origin.as_deref(),
            Some("http://127.0.0.1:6806/")
        );
        assert!(controller.set_origin("https://example.com").is_err());
    }

    #[test]
    fn controller_tracks_ready_and_workspace_mode() {
        let controller = WorkbenchController::new();
        controller.set_origin("http://localhost:6806").unwrap();
        controller.set_ready(true);
        controller.set_mode(WorkspaceMode::Session);

        let status = controller.status();
        assert!(status.available);
        assert!(status.ready);
        assert_eq!(status.mode, WorkspaceMode::Session);
    }
}
