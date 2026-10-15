import { describe, expect, it } from "vitest";
import processing from "../pages/ProcessingPage.tsx?raw";
import voidCommands from "../../src-tauri/src/session_void_commands.rs?raw";
import sink from "../../../../crates/aiks-core/src/sink/siyuan.rs?raw";
import database from "../../../../crates/aiks-core/src/storage/db.rs?raw";
import lexical from "../../../../crates/aiks-core/src/search/lexical.rs?raw";
import semantic from "../../../../crates/aiks-core/src/search/mod.rs?raw";

describe("durable Session void and recovery contract", () => {
  it("keeps Session recoverable but removes managed content and indexes", () => {
    expect(voidCommands).toContain('phase = \'VOIDED\'');
    expect(voidCommands).toContain("DELETE FROM knowledge_item WHERE source_session_id = ?1");
    expect(voidCommands).toContain("DELETE FROM knowledge_chunk");
    expect(voidCommands).toContain("DELETE FROM embedding_record");
    expect(voidCommands).toContain("DELETE FROM session_search_fts");
    expect(voidCommands).toContain("UPDATE source_session SET content_hash = NULL");
    expect(voidCommands).toContain('phase.as_deref() != Some("VOIDED")');
  });
  it("verifies every remote document before deletion and records resumable progress", () => {
    expect(voidCommands).toContain("verify_managed_document_for_removal");
    expect(voidCommands).toContain("session_void_document");
    expect(voidCommands).toContain("mark_phase(&db, session_id, \"CONFLICT\"");
    expect(sink).toContain("remove_managed_document");
    expect(sink).toContain("document {doc_id} has changed");
    expect(database).toContain("CREATE TABLE IF NOT EXISTS session_void_state");
    expect(database).toContain("CREATE TABLE IF NOT EXISTS session_void_document");
  });
  it("blocks search during cleanup and requires explicit restoration", () => {
    expect(lexical).toContain("session_sync_exclusion");
    expect(semantic).toContain("session_sync_exclusion");
    expect(processing).toContain("voidSession(task.session_id)");
    expect(processing).toContain("restoreVoidSession(task.session_id)");
    expect(processing).toContain("syncAndExtract(task.source)");
  });
});
