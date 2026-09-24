import { describe, expect, it, vi } from "vitest";
import { sourceStateLabel, sourceStatusPresentation, sourceFilterOptions, syncCatalogSource, type SourceDescriptor } from "./provider-catalog-model";
import { mockSourceDescriptors } from "./provider-catalog.mock";
import modelSource from "../../../../crates/aiks-core/src/model/mod.rs?raw";
import sourcesSource from "../pages/SourcesPage.tsx?raw";
import sessionsSource from "../pages/SessionsPage.tsx?raw";

const roo: SourceDescriptor = { key: "roo_code", display_name: "Roo Code", config_key: "roo_code", enabled: true, paths: [], status: "ok", message: "OK", restart_required: false };

describe("provider catalog", () => {
  it("sends the stable source key for sync, not a display name", async () => {
    const syncAndExtract = vi.fn().mockResolvedValue({ new_count: 1 });
    await syncCatalogSource({ syncAndExtract }, roo);
    expect(syncAndExtract).toHaveBeenCalledWith("roo_code");
  });
  it("preserves stable values in filters even for disabled historical sources", () => {
    const options = sourceFilterOptions([{ ...roo, enabled: false }]);
    expect(options).toEqual([{ value: "roo_code", label: "Roo Code" }]);
  });
  it("does not sync disabled or unapplied settings", async () => {
    const syncAndExtract = vi.fn();
    await expect(syncCatalogSource({ syncAndExtract }, { ...roo, enabled: false })).rejects.toThrow();
    await expect(syncCatalogSource({ syncAndExtract }, { ...roo, restart_required: true })).rejects.toThrow();
    expect(syncAndExtract).not.toHaveBeenCalled();
  });
  it("keeps healthy, missing, unconfigured and failed stores in a consistent user-facing state model", () => {
    expect(sourceStateLabel(roo)).toBe("可读取");
    expect(sourceStateLabel({ ...roo, status: "not_found" })).toBe("未检测到");
    expect(sourceStateLabel({ ...roo, status: "not_configured" })).toBe("未配置");
    expect(sourceStateLabel({ ...roo, status: "unsupported" })).toBe("读取异常");
    expect(sourceStateLabel({ ...roo, status: "error" })).toBe("读取异常");
    expect(sourceStateLabel({ ...roo, enabled: false })).toBe("已禁用");
    expect(sourceStateLabel({ ...roo, restart_required: true })).toBe("待重启生效");

    expect(sourceStatusPresentation({ ...roo, status: "not_found" })).toMatchObject({
      title: "未检测到本地会话数据",
      tone: "warning",
    });
    expect(sourceStatusPresentation({ ...roo, status: "not_configured" })).toMatchObject({
      title: "尚未配置数据目录",
      tone: "warning",
    });
    expect(sourceStatusPresentation({ ...roo, status: "error" })).toMatchObject({
      title: "本地会话数据读取失败",
      tone: "danger",
    });
  });
  it("mock fixture includes all nineteen Core keys, including the three managed share sources and labels", () => {
    const fixture = mockSourceDescriptors();
    expect(new Set(fixture.map(d => d.key)).size).toBe(19);
    expect(fixture.filter(d => d.configurable !== false)).toHaveLength(16);
    for (const source of fixture) {
      expect(modelSource).toContain(`=> "${source.key}"`);
      expect(modelSource).toContain(`=> "${source.display_name}"`);
    }
    expect(fixture.find(d => d.key === "workbuddy")?.display_name).toBe("WorkBuddy");
  });
  it("both source cards and session filters consume the shared catalog", () => {
    expect(sourcesSource).toContain("useSourceCatalog");
    expect(sourcesSource).toContain("syncCatalogSource");
    expect(sourcesSource).toContain("查看详情");
    expect(sourcesSource).toContain("sourceStatusPresentation");
    expect(sessionsSource).toContain("sourceFilterOptions");
    expect(sessionsSource).toContain("useSourceCatalog");
  });
});
