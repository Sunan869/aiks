import { describe, expect, it, vi } from "vitest";
import { synchronizeEmbeddedLanguage } from "./embedded-siyuan-language";

describe("embedded SiYuan language follows AIKS UI preference", () => {
  it("does not reload or alter SiYuan if its language already matches", async () => {
    const get = vi.fn(async () => "en-US" as const);
    const set = vi.fn(async () => true);
    const reload = vi.fn(async () => {});
    const waitReady = vi.fn(async () => true);
    expect(await synchronizeEmbeddedLanguage("en-US", { get, set, reload, waitReady })).toBe(false);
    expect(set).not.toHaveBeenCalled();
    expect(reload).not.toHaveBeenCalled();
  });

  it("persists requested language, reloads the workbench, and waits for the Bridge", async () => {
    const calls: string[] = [];
    const result = await synchronizeEmbeddedLanguage("en-US", {
      get: async () => { calls.push("get"); return "zh-CN"; },
      set: async lang => { calls.push("set:" + lang); return true; },
      reload: async () => { calls.push("reload"); },
      waitReady: async () => { calls.push("waitReady"); return true; },
    });
    expect(result).toBe(true);
    expect(calls).toEqual(["get", "set:en-US", "reload", "waitReady"]);
  });

  it("does not reload when SiYuan reports no language change", async () => {
    const reload = vi.fn(async () => {});
    expect(await synchronizeEmbeddedLanguage("zh-CN", {
      get: async () => null,
      set: async () => false,
      reload,
      waitReady: async () => true,
    })).toBe(false);
    expect(reload).not.toHaveBeenCalled();
  });

  it("detects a failed Bridge handshake after a language-triggered reload", async () => {
    await expect(synchronizeEmbeddedLanguage("en-US", {
      get: async () => "zh-CN",
      set: async () => true,
      reload: async () => {},
      waitReady: async () => false,
    })).rejects.toThrow("did not become ready");
  });
});
