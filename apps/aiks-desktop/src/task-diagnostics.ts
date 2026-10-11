import type { TaskCenterEntry, TaskCenterStats } from "./api/types";

/** This is an allowlisted, aggregate-only diagnostic, not a log export.
 * Never serialize TaskCenterEntry directly: its title, external_session_id and
 * error strings may contain filenames, prompts, paths, credentials or content.
 */
export interface SafeTaskDiagnostic {
  schema: "aiks-task-diagnostics-v2";
  scope: {
    global_counts: "unbounded";
    sampled_tasks: "latest_200";
    sample_size: number;
    sample_truncated_or_incomplete: boolean;
  };
  counts: {
    total_sessions: number;
    pending: number;
    running: number;
    cancelled: number;
    sync_issues: number;
    ai_issues: number;
  };
  sample: {
    by_source: Record<string, number>;
    sync_state: Record<string, number>;
    ai_state: Record<string, number>;
    issues: Record<string, number>;
    retries: { zero: number; one: number; two_or_more: number; highest_attempt: number };
    stage_latency_ms: { observed: number; p50: number | null; p95: number | null };
  };
  privacy: {
    raw_session_content: false;
    file_paths: false;
    session_identifiers: false;
    titles: false;
    error_messages: false;
    credentials: false;
    model_endpoints: false;
  };
}

const KNOWN_SOURCES = new Set([
  "codex", "claude_code", "opencode", "github_copilot",
  "cursor", "gemini_cli", "qwen", "aider", "cline", "continue",
  "antigravity", "kimi", "workbuddy", "share", "other",
]);

function safeCount(value: number): number {
  return Number.isSafeInteger(value) && value >= 0 ? value : 0;
}

function increment(map: Record<string, number>, key: string): void {
  map[key] = (map[key] || 0) + 1;
}

function syncState(task: TaskCenterEntry): string {
  const status = task.sync_status?.trim().toUpperCase();
  if (status === "SYNCED" || status === "UNCHANGED") return "synced";
  if (status === "CONFLICT") return "conflict";
  if (status?.startsWith("FAILED")) return "failed";
  if (status === "PENDING" || status === "PROCESSING") return "pending";
  return "unavailable_or_other";
}

function aiState(task: TaskCenterEntry): string {
  // Durable generation is authoritative for queued/running/cancelled tasks.
  switch (task.job_status) {
    case "PENDING": return "queued";
    case "RUNNING": return "running";
    case "FAILED": return "failed";
    case "CANCELLED": return "cancelled";
  }
  switch (task.pipeline_status) {
    case "FAILED": return "failed";
    case "READY": return "ready";
    case "PROCESSING": return "running";
    case "RAW_ONLY": return "raw_only";
    case "DISCOVERED": return "queued";
    default: return "unavailable_or_other";
  }
}

function percentile(values: number[], percentileRank: number): number | null {
  if (!values.length) return null;
  return values[Math.ceil(values.length * percentileRank) - 1];
}

/** Use only allowlisted strings and bounded numeric values. No free-form
 * database content, error text or individual Session identity ever leaves.
 */
export function buildSafeTaskDiagnostic(
  stats: TaskCenterStats,
  tasks: readonly TaskCenterEntry[],
): SafeTaskDiagnostic {
  const sample = tasks.slice(0, 200);
  const bySource: Record<string, number> = {};
  const bySync: Record<string, number> = {};
  const byAi: Record<string, number> = {};
  const issues: Record<string, number> = {
    sync_conflict: 0,
    sync_failure: 0,
    ai_failure: 0,
  };
  const retries = { zero: 0, one: 0, two_or_more: 0, highest_attempt: 0 };
  const latency: number[] = [];

  for (const task of sample) {
    const source = KNOWN_SOURCES.has(task.source) ? task.source : "unknown_source";
    increment(bySource, source);
    const sync = syncState(task);
    const ai = aiState(task);
    increment(bySync, sync);
    increment(byAi, ai);
    if (sync === "conflict") issues.sync_conflict += 1;
    if (sync === "failed") issues.sync_failure += 1;
    if (ai === "failed") issues.ai_failure += 1;
    const attempts = safeCount(task.attempts ?? 0);
    if (attempts === 0) retries.zero += 1;
    else if (attempts === 1) retries.one += 1;
    else retries.two_or_more += 1;
    retries.highest_attempt = Math.max(retries.highest_attempt, attempts);
    if (task.stage_latency_ms !== null) {
      const ms = task.stage_latency_ms;
      if (Number.isSafeInteger(ms) && ms >= 0) latency.push(ms);
    }
  }
  latency.sort((a, b) => a - b);

  return {
    schema: "aiks-task-diagnostics-v2",
    scope: {
      global_counts: "unbounded",
      sampled_tasks: "latest_200",
      sample_size: sample.length,
      sample_truncated_or_incomplete: stats.total_sessions > sample.length || tasks.length > 200,
    },
    counts: {
      total_sessions: safeCount(stats.total_sessions),
      pending: safeCount(stats.pending),
      running: safeCount(stats.running),
      cancelled: safeCount(stats.cancelled),
      sync_issues: safeCount(stats.sync_issues),
      ai_issues: safeCount(stats.ai_issues),
    },
    sample: {
      by_source: bySource,
      sync_state: bySync,
      ai_state: byAi,
      issues,
      retries,
      stage_latency_ms: {
        observed: latency.length,
        p50: percentile(latency, 0.5),
        p95: percentile(latency, 0.95),
      },
    },
    privacy: {
      raw_session_content: false,
      file_paths: false,
      session_identifiers: false,
      titles: false,
      error_messages: false,
      credentials: false,
      model_endpoints: false,
    },
  };
}

export function createSafeTaskDiagnosticJson(
  stats: TaskCenterStats,
  tasks: readonly TaskCenterEntry[],
): string {
  return JSON.stringify(buildSafeTaskDiagnostic(stats, tasks), null, 2);
}
