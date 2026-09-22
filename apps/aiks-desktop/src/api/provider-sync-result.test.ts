import { describe, expect, it, vi } from "vitest";
import { MockAiksApi } from "./mock";
import { syncCatalogSource, type SourceDescriptor } from "./provider-catalog-model";
import type { SyncAndExtractResult } from "./types";

const source: SourceDescriptor = {
  key: "roo_code",
  display_name: "Roo Code",
  config_key: "roo_code",
  enabled: true,
  paths: [],
  status: "ok",
  message: "",
};

describe("provider sync result contract", () => {
  it("preserves nonzero failures and sends the stable source key", async () => {
    const result: SyncAndExtractResult = {
      discovered: 4,
      new_count: 1,
      updated_count: 1,
      unchanged_count: 0,
      skipped_count: 0,
      failed_count: 2,
      extraction_queued: 2,
    };
    const api = { syncAndExtract: vi.fn().mockResolvedValue(result) };
    expect(await syncCatalogSource(api, source)).toEqual(result);
    expect(api.syncAndExtract).toHaveBeenCalledTimes(1);
    expect(api.syncAndExtract).toHaveBeenCalledWith("roo_code");
  });

  it("browser mock supplies every counter that the source card consumes", async () => {
    const result = await syncCatalogSource(new MockAiksApi(), source);
    for (const key of ["discovered", "new_count", "updated_count", "unchanged_count", "skipped_count", "failed_count", "extraction_queued"] as const) {
      expect(Number.isFinite(result[key]), key).toBe(true);
      expect(result[key], key).toBeGreaterThanOrEqual(0);
    }
    expect(result.failed_count).toBe(0);
  });
});
