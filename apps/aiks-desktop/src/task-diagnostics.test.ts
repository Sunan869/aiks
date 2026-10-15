import { describe, expect, it } from "vitest";
import type { TaskCenterEntry, TaskCenterStats } from "./api/types";
import { buildSafeTaskDiagnostic, createSafeTaskDiagnosticJson } from "./task-diagnostics";

const stats: TaskCenterStats = {
  total_sessions: 300, pending: 11, running: 2, cancelled: 3, sync_issues: 9, ai_issues: 4,
};

function task(overrides: Partial<TaskCenterEntry> = {}): TaskCenterEntry {
  return {
    session_id: 10,
    source: "codex",
    external_session_id: "secret-session",
    title: "private ticket contents",
    sync_status: "SYNCED",
    sync_error: null,
    pipeline_status: "READY",
    current_stage: "INDEXED",
    pipeline_error: null,
    job_status: "DONE",
    attempts: 1,
    job_error: null,
    stage_latency_ms: 500,
    last_task_update: "2026-10-10T13:30:00Z",
    sync_excluded: false,
    ...overrides,
  };
}

describe("S1.2 privacy-safe task diagnostic snapshot", () => {
  it("never serializes session titles, paths, errors, credentials or free text", () => {
    const privateText = "Token Bearer abc123 /home/private/project/user.json prompt raw-content";
    const input = [task({
      title: privateText, external_session_id: privateText, source: privateText,
      sync_error: privateText, pipeline_error: privateText, job_error: privateText,
      current_stage: privateText, last_task_update: privateText,
    })];
    const result = createSafeTaskDiagnosticJson(stats, input);
    expect(result).not.toContain("abc123");
    expect(result).not.toContain("private");
    expect(result).not.toContain("project/user");
    expect(result).not.toContain("raw-content");
    expect(result).not.toContain("secret-session");
    expect(result).not.toContain("Bearer");
    expect(result).toContain("unknown_source");
    const structured = JSON.parse(result);
    expect(structured.privacy).toEqual({
      raw_session_content: false, file_paths: false, session_identifiers: false,
      titles: false, error_messages: false, credentials: false, model_endpoints: false,
    });
    expect(structured).not.toHaveProperty("tasks");
  });

  it("counts global vs bounded recent task sample separately", () => {
    const input = [
      task(),
      task({ sync_status: "CONFLICT", pipeline_status: "FAILED", job_status: "FAILED", attempts: 3, stage_latency_ms: 1500 }),
      task({ source: "claude_code", sync_status: "FAILED_RETRYABLE", job_status: "RUNNING", attempts: 2, stage_latency_ms: 250 }),
      task({ source: "opencode", sync_status: "PENDING", job_status: "PENDING", attempts: 0, stage_latency_ms: null }),
      task({ source: "fake", sync_status: "CUSTOM_TOKEN_STRING", job_status: "CUSTOM", pipeline_status: "CUSTOM", attempts: -10 }),
    ];
    const result = buildSafeTaskDiagnostic(stats, input);
    expect(result.counts).toEqual(stats);
    expect(result.scope).toEqual({
      global_counts: "unbounded", sampled_tasks: "latest_200",
      sample_size: 5, sample_truncated_or_incomplete: true,
    });
    expect(result.sample.sync_state).toEqual({
      synced: 1, conflict: 1, failed: 1, pending: 1, unavailable_or_other: 1,
    });
    expect(result.sample.ai_state).toEqual({
      ready: 1, failed: 1, running: 1, queued: 1, unavailable_or_other: 1,
    });
    expect(result.sample.issues).toEqual({ sync_conflict: 1, sync_failure: 1, ai_failure: 1 });
    expect(result.sample.retries).toEqual({ zero: 2, one: 1, two_or_more: 2, highest_attempt: 3 });
    expect(result.sample.stage_latency_ms).toEqual({ observed: 4, p50: 500, p95: 1500 });
    expect(result.sample.by_source).toEqual({
      codex: 2, claude_code: 1, opencode: 1, unknown_source: 1,
    });
  });

  it("caps sampled records and never trusts invalid count or latency values", () => {
    const input = Array.from({ length: 800 }, (_, i) => task({
      source: i % 2 ? "codex" : "unknown/secret",
      stage_latency_ms: i === 0 ? Number.POSITIVE_INFINITY : i,
      attempts: i === 1 ? Number.MAX_VALUE : 1,
    }));
    const result = buildSafeTaskDiagnostic({ ...stats, total_sessions: -7 }, input);
    expect(result.scope.sample_size).toBe(200);
    expect(result.scope.sample_truncated_or_incomplete).toBe(true);
    expect(result.counts.total_sessions).toBe(0);
    expect(result.sample.stage_latency_ms.observed).toBe(199);
    expect(result.sample.retries.highest_attempt).toBe(1);
    expect(Object.values(result.sample.by_source).reduce((n, count) => n + count, 0)).toBe(200);
  });

  it("returns an empty, valid explicit snapshot for an empty database", () => {
    const result = buildSafeTaskDiagnostic({ ...stats, total_sessions: 0 }, []);
    expect(result.scope.sample_size).toBe(0);
    expect(result.scope.sample_truncated_or_incomplete).toBe(false);
    expect(result.sample.stage_latency_ms).toEqual({ observed: 0, p50: null, p95: null });
  });
});
