import { describe, expect, it, vi } from "vitest";
import { fetchVerifiedProcessingDetail } from "./processing-detail-refresh";
import type { PipelineSummary } from "./api/types";
import pageSource from "./pages/ProcessingDetailPage.tsx?raw";
import centerSource from "./pages/ProcessingPage.tsx?raw";

function detail(run_id = "run-1", session_id = 42): PipelineSummary {
  return {
    run_id, session_id, session_title: "Sample", source: "codex", status: "FAILED",
    current_stage: "AI_EXTRACTED", pipeline_version: "test", started_at: null,
    finished_at: null, error_stage: null, error_message: null,
    stage_runs: [], knowledge_count: 0,
  };
}

describe("S1.2 processing detail route freshness", () => {
  it("accepts a matching run with a valid original Session", async () => {
    const fetch = vi.fn(async () => detail());
    expect(await fetchVerifiedProcessingDetail(fetch, "run-1")).toEqual({
      status: "ok", value: detail(),
    });
    expect(fetch).toHaveBeenCalledWith("run-1");
  });

  it("rejects mismatched or malformed response identity without exposing it", async () => {
    for (const bad of [detail("other-run"), detail("run-1", 0),
      detail("run-1", Number.NaN), detail("run-1", -2)]) {
      expect(await fetchVerifiedProcessingDetail(async () => bad, "run-1"))
        .toEqual({ status: "unavailable" });
    }
    expect(await fetchVerifiedProcessingDetail(async () => detail(), "  "))
      .toEqual({ status: "unavailable" });
  });

  it("discards arbitrary transport exceptions and does not retry automatically", async () => {
    const fetch = vi.fn(async (): Promise<PipelineSummary> => {
      throw new Error("Bearer SECRET from /private/path");
    });
    const result = await fetchVerifiedProcessingDetail(fetch, "run-1");
    expect(result).toEqual({ status: "unavailable" });
    expect(JSON.stringify(result)).not.toMatch(/SECRET|private/);
    expect(fetch).toHaveBeenCalledTimes(1);
  });

  it("keeps route generation, identity and mutation safeguards in both screens", () => {
    expect(pageSource).toContain("requestGeneration.current !== requestId");
    expect(pageSource).toContain("loadedRunId !== runId");
    expect(pageSource).toContain("submitProcessingDetailAction(getApi(), detail)");
    expect(pageSource).not.toContain("setRetryMsg(`错误：${e}`)");
    expect(centerSource).toContain("collectProcessingPanels(getApi())");
    expect(centerSource).toContain("isFreshProcessingRead(startedGeneration");
    expect(centerSource).toContain("mutationGeneration.current += 1");
    expect(centerSource).toContain("!taskPanelFresh");
    expect(centerSource).not.toContain("setLoadError(\"无法刷新任务状态：\" + String(error))");
  });
});
