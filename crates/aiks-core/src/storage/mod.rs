pub mod db;
pub mod repo;

pub use db::StateDb;
pub use repo::*;

/// B14: Rebuild only the sync index tables, preserving all V3 knowledge data.
///
/// Resets: sync_target, sync_run, source_file_state
/// Preserves: source_session (metadata), knowledge_item, knowledge_chunk,
///            embedding_record, pipeline_run, pipeline_stage_run, session_chunk
pub fn rebuild_sync_index_only(db: &StateDb) -> anyhow::Result<()> {
    let conn = db.conn();
    conn.execute_batch(
        "
        DELETE FROM sync_target;
        DELETE FROM sync_run;
        DELETE FROM source_file_state;
        -- Reset content hashes so all sessions will re-sync
        UPDATE source_session SET content_hash = NULL, updated_at = datetime('now');
    ",
    )?;
    tracing::info!("[REBUILD] Sync index cleared; knowledge data preserved");
    Ok(())
}
