#[cfg(test)]
mod tests {
    use aiks_core::knowledge::ContentMigrationStats;

    use crate::workbench::controller::WorkbenchStatus;
    use crate::workbench::protocol::WorkspaceMode;

    #[test]
    fn aggregates_siyuan_workbench_bridge_and_migration_status() {
        let workbench = WorkbenchStatus {
            available: true,
            ready: true,
            mode: WorkspaceMode::Session,
            origin: Some("http://127.0.0.1:6806/".into()),
            protocol_version: 1,
        };
        let migration = ContentMigrationStats {
            total: 7,
            pending: 2,
            migrated: 2,
            reused: 1,
            conflicts: 1,
            failed: 1,
        };

        let diagnostics = super::compose_v41_diagnostics(true, workbench, migration);

        assert!(diagnostics.siyuan_ready);
        assert!(diagnostics.workbench.available);
        assert!(diagnostics.workbench.ready);
        assert_eq!(diagnostics.workbench.mode, WorkspaceMode::Session);
        assert_eq!(diagnostics.workbench.protocol_version, 1);
        assert_eq!(diagnostics.migration.total, 7);
        assert_eq!(diagnostics.migration.pending, 2);
        assert_eq!(diagnostics.migration.migrated, 2);
        assert_eq!(diagnostics.migration.reused, 1);
        assert_eq!(diagnostics.migration.conflicts, 1);
        assert_eq!(diagnostics.migration.failed, 1);
    }
}
