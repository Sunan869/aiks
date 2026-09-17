import { describe, expect, it } from "vitest";
import { MAIN_NAV_PAGE_IDS } from "./Sidebar";

describe("V4.2 sidebar navigation", () => {
  it("keeps user-facing search inside the Knowledge Workbench", () => {
    expect(MAIN_NAV_PAGE_IDS).toEqual([
      "overview",
      "sessions",
      "knowledge",
      "processing",
    ]);
    expect(MAIN_NAV_PAGE_IDS).not.toContain("search");
  });
});
