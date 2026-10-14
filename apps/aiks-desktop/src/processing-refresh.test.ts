import { describe, expect, it, vi } from "vitest";
import { collectProcessingPanels, isFreshProcessingRead } from "./processing-refresh";
import type { ProcessingReaders } from "./processing-refresh";
import type { PipelineStats, TaskCenterStats } from "./api/types";

const pipelineStats: PipelineStats = {
  total: 2, processing: 0, ready: 1, raw_only: 0, failed: 1,
  knowledge_items: 1, knowledge_chunks: 1, embeddings: 1,
};
const taskStats: TaskCenterStats = {
  total_sessions: 400, pending: 2, running: 1, cancelled: 0, sync_issues: 1, ai_issues: 2,
};
function readers(): ProcessingReaders {
  return {
    getPipelineRuns: vi.fn(async () => []),
    getPipelineStats: vi.fn(async () => pipelineStats),
    getTaskCenterEntries: vi.fn(async () => []),
    getTaskCenterStats: vi.fn(async () => taskStats),
  };
}

describe("S1.2 processing panels fail independently and retain coherent pairs", () => {
  it("reads both bounded lists with their independent global counts", async () => {
    const api = readers();
    const result = await collectProcessingPanels(api);
    expect(result.tasks).toEqual({ status: "ok", first: [], second: taskStats });
    expect(result.pipeline).toEqual({ status: "ok", first: [], second: pipelineStats });
    expect(api.getTaskCenterEntries).toHaveBeenCalledWith(200);
    expect(api.getPipelineRuns).toHaveBeenCalledWith(300);
  });

  it("rejects a partial task pair while preserving a successful pipeline pair", async () => {
    const api = readers();
    vi.mocked(api.getTaskCenterStats).mockRejectedValueOnce(new Error("Bearer SECRET /private"));
    const result = await collectProcessingPanels(api);
    expect(result.tasks).toEqual({ status: "error" });
    expect(result.pipeline.status).toBe("ok");
    expect(JSON.stringify(result)).not.toContain("SECRET");
  });

  it("rejects a partial pipeline pair but still returns current task data", async () => {
    const api = readers();
    vi.mocked(api.getPipelineRuns).mockRejectedValueOnce(new Error("sensitive path"));
    const result = await collectProcessingPanels(api);
    expect(result.pipeline).toEqual({ status: "error" });
    expect(result.tasks).toEqual({ status: "ok", first: [], second: taskStats });
  });

  it("isolates synchronous adapter throws without leaking exception content", async () => {
    const api = readers();
    api.getTaskCenterEntries = () => { throw new Error("secret-session-id"); };
    api.getPipelineStats = () => { throw new Error("private-url"); };
    const result = await collectProcessingPanels(api);
    expect(result).toEqual({ tasks: { status: "error" }, pipeline: { status: "error" } });
    expect(JSON.stringify(result)).not.toMatch(/secret-session-id|private-url/);
  });

  it("rejects old polls and concurrent writes even if the API succeeded", () => {
    expect(isFreshProcessingRead(3, 3, false)).toBe(true);
    expect(isFreshProcessingRead(2, 3, false)).toBe(false);
    expect(isFreshProcessingRead(3, 3, true)).toBe(false);
    expect(isFreshProcessingRead(4, 5, false)).toBe(false);
  });
});
