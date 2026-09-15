import { afterEach, describe, expect, it, vi } from "vitest";
import * as apiModule from "./index";

const { isTauriContext, shouldUseMock } = apiModule;

describe("API runtime selection", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("uses the mock API outside a Tauri runtime", () => {
    vi.stubGlobal("window", {});
    expect(isTauriContext()).toBe(false);
    expect(shouldUseMock()).toBe(true);
  });

  it("uses the real API when Tauri internals are present", () => {
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
    expect(isTauriContext()).toBe(true);
    expect(shouldUseMock()).toBe(false);
  });
});

describe("SiYuan workspace URL policy", () => {
  it("accepts only loopback HTTP(S) runtime URLs and normalizes them to an origin", () => {
    const normalize = (apiModule as unknown as Record<string, unknown>).normalizeSiyuanWorkspaceUrl;
    expect(normalize).toEqual(expect.any(Function));
    if (typeof normalize !== "function") return;

    const call = normalize as (value: string | null) => string | null;
    expect(call("http://127.0.0.1:6812/")).toBe("http://127.0.0.1:6812");
    expect(call("http://localhost:6806/stage/build/desktop/?foo=bar#hash")).toBe("http://localhost:6806");
    expect(call("https://[::1]:6806/")).toBe("https://[::1]:6806");
    expect(call("https://example.com:6806/")).toBeNull();
    expect(call("file:///tmp/siyuan")).toBeNull();
    expect(call("not a url")).toBeNull();
    expect(call(null)).toBeNull();
  });
});
