import { describe, expect, it } from "vitest";
import { allOrganizationSourcesReviewed, reviewOrganizationSource } from "./knowledge-organization-review";
import page from "./pages/KnowledgeDetailPage.tsx?raw";

describe("S3.4 multi-source human evidence review checkpoint", () => {
  it("requires every loaded source, in reviewed snapshot order, before confirming", () => {
    const expected = ["kn_0001", "kn_0005", "k3"];
    expect(allOrganizationSourcesReviewed(expected, expected, ["kn_0001"])).toBe(false);
    let visited = ["kn_0001"];
    visited = reviewOrganizationSource(expected, visited, "kn_0005");
    expect(allOrganizationSourcesReviewed(expected, expected, visited)).toBe(false);
    visited = reviewOrganizationSource(expected, visited, "k3");
    expect(allOrganizationSourcesReviewed(expected, expected, visited)).toBe(true);
    expect(reviewOrganizationSource(expected, visited, "k3")).toEqual(visited);
    expect(allOrganizationSourcesReviewed(["one"], ["one"], ["one"])).toBe(true);
  });

  it("rejects stale source snapshots, duplicates and injected IDs", () => {
    expect(allOrganizationSourcesReviewed(["a", "b"], ["b", "a"], ["a", "b"])).toBe(false);
    expect(allOrganizationSourcesReviewed(["a", "b"], ["a"], ["a", "b"])).toBe(false);
    expect(allOrganizationSourcesReviewed(["a", "a"], ["a", "a"], ["a"])).toBe(false);
    expect(allOrganizationSourcesReviewed(["a", "b"], ["a", "b"], ["a", "a", "b"])).toBe(false);
    expect(allOrganizationSourcesReviewed(["a", "b"], ["a", "b"], ["a", "b", "foreign"])).toBe(false);
    expect(allOrganizationSourcesReviewed([], [], [])).toBe(false);
    expect(allOrganizationSourcesReviewed(
      ["a", "b", "c", "d", "e", "f"], ["a", "b", "c", "d", "e", "f"], ["a", "b", "c", "d", "e", "f"],
    )).toBe(false);
    expect(() => reviewOrganizationSource(["a", "b"], ["a"], "foreign")).toThrow();
    expect(() => reviewOrganizationSource(["a", "a"], [], "a")).toThrow();
  });

  it("has a user-visible per-source diff and resets confirmation when the draft changes", () => {
    expect(page).toContain("逐一审阅全部来源知识");
    expect(page).toContain("reviewedSources.map(source =>");
    expect(page).toContain("setDiffSourceId(source.id)");
    expect(page).toContain("reviewOrganizationSource(organizeSourceIds, current, source.id)");
    expect(page).toContain("previewKnowledgeOrganizationDiff(diffReferenceContent, organizeDraft)");
    expect(page).toContain("organizeBusy || !sourcesReviewed");
    expect(page).toContain("setSourceReviewVisitedIds(organizeSourceIds.slice(0, 1))");
    expect(page).toContain("setReviewedSources(sources)");
    expect(page).toContain("setReviewedSources([])");
    expect(page).toContain("await getApi().validateOrganizationScope(currentSources.map(source => source.id))");
    expect(page).toContain("organizationSourceSnapshot(currentSources) !== organizeSourceSnapshot");
    expect(page).not.toContain("applyOrganizationDiffToSiyuan");
  });
});
