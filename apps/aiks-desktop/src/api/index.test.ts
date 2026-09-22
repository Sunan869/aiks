import { afterEach, describe, expect, it, vi } from "vitest";
import { isTauriContext, shouldUseMock } from "./index";

describe("API runtime selection", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.unstubAllEnvs();
  });

  it("does not silently use mock API outside a Tauri runtime", () => {
    vi.stubGlobal("window", {});
    expect(isTauriContext()).toBe(false);
    expect(shouldUseMock()).toBe(false);
  });

  it("permits mock only through explicit development opt-in", () => {
    vi.stubEnv("DEV", true); vi.stubEnv("VITE_AIKS_MOCK", "true");
    expect(shouldUseMock()).toBe(true);
    vi.stubEnv("DEV", false);
    expect(shouldUseMock()).toBe(false);
  });

  it("uses the real API when Tauri internals are present", () => {
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
    expect(isTauriContext()).toBe(true);
    expect(shouldUseMock()).toBe(false);
  });
});
