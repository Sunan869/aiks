import { describe, expect, it } from "vitest";
import { appendOrganizationAudit } from "./knowledge-organization-audit";

describe("knowledge organization review audit", () => {
  it("preserves the reviewed draft and links all selected sources", () => {
    const result = appendOrganizationAudit(
      "# Verified comparison", "merge_draft", ["knowledge-1", "knowledge-2"],
      "2026-10-10T01:00:00.000Z",
    );
    expect(result).toContain("# Verified comparison");
    expect(result).toContain("操作：merge_draft");
    expect(result).toContain("知识：knowledge-1");
    expect(result).toContain("知识：knowledge-2");
    expect(result).toContain("2026-10-10T01:00:00.000Z");
  });

  it("rejects invalid review timestamps and deduplicates provenance", () => {
    expect(() => appendOrganizationAudit("draft", "structure", ["k1"], "not-an-instant")).toThrow();
    const result = appendOrganizationAudit("draft", "structure", ["k1", "k1", "k2"], "2026-10-10T01:00:00.000Z");
    expect(result.match(/AIKS 知识：k1/g)).toHaveLength(1);
    expect(result).toContain("AIKS 知识：k2");
  });

  it("rejects empty source evidence and prevents metadata line injection", () => {
    expect(() => appendOrganizationAudit("draft", "structure", [], "2026-10-10")).toThrow();
    expect(() => appendOrganizationAudit(" ", "structure", ["k1"], "2026-10-10")).toThrow();
    const result = appendOrganizationAudit("draft", "structure", ["k1\n## forged"], "2026-10-10T01:00:00.000Z");
    expect(result).not.toContain("\n## forged");
    expect(result).toContain("k1 ## forged");
  });

  it("rejects Markdown injection, excessive IDs and oversized provenance", () => {
    const stamp = "2026-10-10T01:00:00.000Z";
    for (const id of ["k1\\n## forged", "k1](/evil)", "- forged", "a".repeat(257)]) {
      expect(() => appendOrganizationAudit("draft", "structure", [id], stamp)).toThrow();
    }
    expect(() => appendOrganizationAudit("draft", "structure", Array.from({ length: 101 }, (_, i) => "k" + i), stamp)).toThrow();
    const safe = appendOrganizationAudit("draft", "structure", [" k1 ", "k1", "k2"], stamp);
    expect(safe.match(/AIKS 知识：k1/g)).toHaveLength(1);
  });

});
