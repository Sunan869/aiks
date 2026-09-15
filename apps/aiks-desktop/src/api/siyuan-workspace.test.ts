import { describe, expect, it } from "vitest";
import { MockAiksApi } from "./mock";

describe("SiYuan workspace API", () => {
  it("exposes one Desktop entry point for the native SiYuan workspace", () => {
    const api = new MockAiksApi() as unknown as Record<string, unknown>;
    expect(api.openSiyuanWorkspace).toEqual(expect.any(Function));
  });
});
