import { describe, expect, it } from "vitest";
import {
  BOTTOM_NAV_ITEMS,
  MAIN_NAV_ITEMS,
  rawConversationNavState,
} from "./navigation";

describe("V4.2 product navigation", () => {
  it("removes the duplicate user-facing Search page", () => {
    expect(MAIN_NAV_ITEMS.map(item => item.id)).toEqual([
      "overview",
      "sessions",
      "knowledge",
      "processing",
    ]);
    expect(
      [...MAIN_NAV_ITEMS, ...BOTTOM_NAV_ITEMS].some(item => String(item.id) === "search"),
    ).toBe(false);
  });

  it("routes raw conversations into Knowledge session mode", () => {
    expect(rawConversationNavState("session-doc-1")).toEqual({
      page: "knowledge",
      workbenchMode: "session",
      workbenchDocId: "session-doc-1",
    });
    expect(rawConversationNavState(null)).toEqual({
      page: "knowledge",
      workbenchMode: "session",
    });
  });
});
