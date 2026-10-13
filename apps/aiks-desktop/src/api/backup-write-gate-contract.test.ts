import { describe, expect, it } from "vitest";
import knowledge from "../../src-tauri/src/knowledge_commands.rs?raw";
import project from "../../src-tauri/src/project_memory_commands.rs?raw";
import manual from "../../src-tauri/src/commands.rs?raw";

function functionBody(source: string, name: string): string {
  const start = source.indexOf("pub async fn " + name + "(");
  expect(start, name + " must exist").toBeGreaterThanOrEqual(0);
  const next = source.indexOf("#[tauri::command]", start + 1);
  return source.slice(start, next < 0 ? source.length : next);
}

describe("S1.3 desktop write gate contracts", () => {
  const commands = [
    [knowledge, [
      "create_knowledge", "create_organized_knowledge", "set_knowledge_favorite",
      "archive_knowledge", "restore_knowledge", "publish_knowledge",
      "add_knowledge_feedback", "suggest_knowledge_relation", "review_knowledge_relation",
    ]],
    [project, [
      "bind_project_session", "clear_project_session_binding", "create_project_review",
      "save_project_report_draft",
    ]],
    [manual, [
      "sync_now", "sync_and_extract", "sync_knowledge_to_siyuan",
      "extract_session_now", "retry_failed_ai_task", "cancel_pending_ai_task",
      "run_pipeline_for_session", "backfill_extractions",
    ]],
  ] as const;

  it("requires an acquired permit at each audited mutating Tauri boundary", () => {
    for (const [source, names] of commands) {
      for (const name of names) {
        const body = functionBody(source, name);
        expect(body, name).toContain("BackupQuiescence");
        expect(body, name).toContain(".try_begin_write()");
        expect(body, name).toContain("let _writer = gate");
      }
    }
  });

  it("forwards the gate to nested manual creation", () => {
    expect(functionBody(knowledge, "create_organized_knowledge"))
      .toContain("create_knowledge(input, state, gate.clone()).await");
  });
});
