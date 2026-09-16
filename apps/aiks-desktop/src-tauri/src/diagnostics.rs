use aiks_core::knowledge::{ContentMigrationService, ContentMigrationStats};
use serde::Serialize;
use tauri::State;

use crate::app_state::AppState;
use crate::workbench::controller::{WorkbenchController, WorkbenchStatus};

#[derive(Debug, Clone, Serialize)]
pub struct V41Diagnostics {
    pub siyuan_ready: bool,
    pub workbench: WorkbenchStatus,
    pub migration: ContentMigrationStats,
}

pub fn compose_v41_diagnostics(
    siyuan_ready: bool,
    workbench: WorkbenchStatus,
    migration: ContentMigrationStats,
) -> V41Diagnostics {
    V41Diagnostics {
        siyuan_ready,
        workbench,
        migration,
    }
}

#[tauri::command]
pub async fn get_v41_diagnostics(
    state: State<'_, AppState>,
    workbench: State<'_, WorkbenchController>,
) -> Result<V41Diagnostics, String> {
    let siyuan_ready = state.siyuan_url().await.is_some();
    let workbench_status = workbench.status();
    let migration = match state.engine() {
        Some(engine) => ContentMigrationService::new(engine.db().as_ref())
            .stats()
            .map_err(|error| error.to_string())?,
        None => ContentMigrationStats::default(),
    };

    Ok(compose_v41_diagnostics(
        siyuan_ready,
        workbench_status,
        migration,
    ))
}

#[cfg(test)]
mod tests {
    use aiks_core::knowledge::ContentMigrationStats;

    use crate::workbench::controller::WorkbenchStatus;
    use crate::workbench::protocol::WorkspaceMode;

    #[test]
    fn bundled_siyuan_runtime_is_pinned() {
        let manifest: serde_json::Value = serde_json::from_str(include_str!(
            "../resources/siyuan-runtime.json"
        ))
        .unwrap();
        assert_eq!(manifest["siyuanBaseVersion"], "3.8.3");
        assert_eq!(
            manifest["siyuanUpstreamCommit"],
            "8641553a1f07374001902d3ce773285db1292b2d"
        );
        assert_eq!(manifest["bridgeProtocolVersion"], 2);
    }

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
