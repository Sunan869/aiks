import { describe, expect, it } from "vitest";
import workspace from "../components/ui/workspace.tsx?raw";
import overview from "../pages/OverviewPage.tsx?raw";
import sync from "../pages/SyncPage.tsx?raw";
import sources from "../pages/SourcesPage.tsx?raw";
describe("responsive dashboard grid contract", () => {
  it("encodes desktop breakpoints once in a reusable primitive", () => {
    expect(workspace).toContain('4: "sm:grid-cols-4"');
    expect(workspace).toContain('6: "sm:grid-cols-3 xl:grid-cols-6"');
    expect(workspace).toContain("grid-cols-2");
  });
  it("keeps overview, synchronization and sources on the common grid", () => {
    for (const page of [overview, sync, sources]) expect(page).toContain("<UiStatGrid>");
    expect(sync.match(/<UiStatGrid>/g)).toHaveLength(2);
  });
});
