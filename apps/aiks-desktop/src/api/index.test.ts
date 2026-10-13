import { afterEach, describe, expect, it, vi } from "vitest";
import { isTauriContext, shouldUseMock } from "./index";

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
