import { describe, expect, it, vi } from "vitest";
import { submitProcessingDetailAction } from "./processing-detail-action";
import source from "./pages/ProcessingDetailPage.tsx?raw";
import rustSource from "../src-tauri/src/commands.rs?raw";
import tauriSource from "./api/tauri.ts?raw";
import coreGuard from "../../../crates/aiks-core/src/pipeline/first_run.rs?raw";

function api() {
  return {
    retryFailedAiTask: vi.fn(async (_id: number) => "retry-job"),
    runPipelineForSession: vi.fn(async (_id: number) => "first-job"),
  };
}

describe("S1.2 historical processing detail safely enqueues work", () => {
  it("sends FAILED only to retry_failed_ai_task, never the legacy first-run path", async () => {
    const client = api();
    expect(await submitProcessingDetailAction(client, { status: "FAILED", session_id: 4 })).toBe("retry");
    expect(client.retryFailedAiTask).toHaveBeenCalledTimes(1);
    expect(client.retryFailedAiTask).toHaveBeenCalledWith(4);
    expect(client.runPipelineForSession).not.toHaveBeenCalled();
  });

  it("sends RAW_ONLY only to the guarded first processing route", async () => {
    const client = api();
    expect(await submitProcessingDetailAction(client, { status: "RAW_ONLY", session_id: 9 })).toBe("first_run");
    expect(client.runPipelineForSession).toHaveBeenCalledTimes(1);
    expect(client.runPipelineForSession).toHaveBeenCalledWith(9);
    expect(client.retryFailedAiTask).not.toHaveBeenCalled();
  });

  it("rejects stale, ready, processing or invalid session identities without enqueuing", async () => {
    const client = api();
    for (const status of ["READY", "PROCESSING", "CANCELLED", "CONFLICT", "UNKNOWN"]) {
      await expect(submitProcessingDetailAction(client, { status, session_id: 2 })).rejects.toThrow();
    }
    for (const session_id of [0, -1, Number.NaN, Number.POSITIVE_INFINITY, 1.5]) {
      await expect(submitProcessingDetailAction(client, { status: "FAILED", session_id })).rejects.toThrow();
    }
    expect(client.retryFailedAiTask).not.toHaveBeenCalled();
    expect(client.runPipelineForSession).not.toHaveBeenCalled();
  });

  it("does not automatically fall back to unguarded processing when retry fails", async () => {
    const client = api();
    client.retryFailedAiTask.mockRejectedValueOnce(new Error("Unsafe historical retry"));
    await expect(submitProcessingDetailAction(client, { status: "FAILED", session_id: 5 }))
      .rejects.toThrow("Unsafe historical retry");
    expect(client.runPipelineForSession).not.toHaveBeenCalled();
  });

  it("keeps both UI and Tauri boundaries using the same durable queue checks", () => {
    expect(source).toContain("submitProcessingDetailAction(getApi(), detail)");
    expect(source).not.toContain('invoke("run_pipeline_for_session"');
    expect(tauriSource).toContain('invoke("retry_failed_ai_task", { sessionId })');
    expect(tauriSource).toContain('invoke("run_pipeline_for_session", { sessionId })');
    expect(rustSource).toContain("pub async fn retry_failed_ai_task(");
    expect(rustSource).toContain("pub async fn run_pipeline_for_session(");
    expect(rustSource).toContain("first_run::load_initial_pipeline_session(&db, session_id)");
    expect(coreGuard).toContain("JOIN sync_target st ON st.session_id = ss.id AND st.sink = 'siyuan'");
    expect(coreGuard).toContain("st.status IN ('SYNCED','UNCHANGED')");
    expect(coreGuard).toContain("pj.status IN ('PENDING','RUNNING')");
    expect(coreGuard).toContain("pr.status <> 'RAW_ONLY'");
    expect(coreGuard).toContain("ss.is_missing = 0");
    expect(coreGuard).toContain("only_synced_raw_source_without_prior_failed_ready_or_active_work_is_eligible");
  });
});
