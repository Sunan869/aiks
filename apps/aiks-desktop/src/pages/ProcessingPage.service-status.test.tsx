import { beforeEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import type { PipelineSummary } from "../api/types";
import ProcessingPage from "./ProcessingPage";

const seed = vi.hoisted(() => ({ status: "SUPERSEDED" }));

vi.mock("../ProviderCatalog", () => ({ useSourceName: () => (value: string) => value }));
vi.mock("../api/client", () => ({ getApi: () => { throw new Error("SSR must not call APIs"); } }));
vi.mock("react", async (original) => {
  const react = await original<typeof import("react")>();
  return {
    ...react,
    // Supply the fetched state to the real page's SSR render; no DOM, network,
    // effects, or replacement status renderer is involved in this contract test.
    useState: (initial: unknown) => react.useState(Array.isArray(initial) ? [{
      run_id: "historical-run", session_id: 1, session_title: "Historical revision",
      source: "continue", status: seed.status, current_stage: null,
      pipeline_version: "service-v1/r1", started_at: null, finished_at: null,
      error_stage: null, error_message: null, stage_runs: [], knowledge_count: 0,
    } satisfies PipelineSummary] : initial === true ? false : initial),
  };
});

describe("service revision status in the existing processing page", () => {
  beforeEach(() => { seed.status = "SUPERSEDED"; });

  it("renders a superseded historical run without classifying it as complete", () => {
    const html = renderToStaticMarkup(<ProcessingPage />);
    expect(html).toContain("Historical revision");
    expect(html).toContain("SUPERSEDED");
    expect(html).toContain("text-xs font-medium text-gray-500");
    expect(html).not.toContain("text-xs font-medium text-green-600");
  });

  it("continues rendering valid completed runs as completed", () => {
    seed.status = "READY";
    const html = renderToStaticMarkup(<ProcessingPage />);
    expect(html).toContain("text-xs font-medium text-green-600");
    expect(html).not.toContain("SUPERSEDED");
  });
});
