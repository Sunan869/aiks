import { describe, expect, it } from "vitest";
import { rawConversationNavState } from "./navigation";

describe("V4.2 raw conversation navigation", () => {
  it("normalizes a bound SiYuan document id before entering Knowledge", () => {
    expect(rawConversationNavState("  session-doc-2  ")).toEqual({
      page: "knowledge",
      workbenchMode: "session",
      workbenchDocId: "session-doc-2",
    });
  });
});
