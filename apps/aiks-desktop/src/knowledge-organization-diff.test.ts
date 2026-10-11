import { describe, expect, it } from "vitest";
import { previewKnowledgeOrganizationDiff } from "./knowledge-organization-diff";
import page from "./pages/KnowledgeDetailPage.tsx?raw";

describe("S3.4 user-reviewed independent organization diff preview", () => {
  it("represents changed, inserted, deleted and unchanged lines without modifying inputs", () => {
    const source = "First line\nStay\nDelete me\nLast";
    const draft = "First line\nStay\nNew point\nLast";
    const diff = previewKnowledgeOrganizationDiff(source, draft);
    expect(diff.truncated).toBe(false);
    expect(diff.identical).toBe(false);
    expect(diff.rows).toEqual([
      { kind: "same", text: "First line" },
      { kind: "same", text: "Stay" },
      { kind: "removed", text: "Delete me" },
      { kind: "added", text: "New point" },
      { kind: "same", text: "Last" },
    ]);
    expect(diff.added).toBe(1);
    expect(diff.removed).toBe(1);
    expect(source).toContain("Delete me");
    expect(draft).not.toContain("Delete me");
  });

  it("has an explicit no-change state and treats Windows LF/CRLF as equivalent", () => {
    const a = previewKnowledgeOrganizationDiff("A\r\nB", "A\nB");
    expect(a.identical).toBe(true);
    expect(a.added).toBe(0);
    expect(a.removed).toBe(0);
    const b = previewKnowledgeOrganizationDiff("A\nB\n", "A\nB");
    expect(b.identical).toBe(false);
    expect(b.removed).toBe(1);
  });

  it("explicitly refuses to imply complete review of oversized or adversarial multiline content", () => {
    const many = "x\n".repeat(1400);
    const diff = previewKnowledgeOrganizationDiff(many, "x\n".repeat(1400));
    expect(diff.truncated).toBe(true);
    expect(diff.rows.length).toBeLessThanOrEqual(500);
    expect(diff.identical).toBe(true);
    const malicious = previewKnowledgeOrganizationDiff(
      "unchanged", "<script>alert('injected')</script>\n" + "bad".repeat(4000),
    );
    expect(malicious.truncated).toBe(true);
    expect(malicious.rows.some(row => row.text.includes("<script>"))).toBe(true);
    expect(malicious.rows.every(row => [...row.text].length <= 825)).toBe(true);
    expect(previewKnowledgeOrganizationDiff("x", "y").added).toBe(1);
  });

  it("keeps large-but-bounded diffs deterministic and lossless within preview limits", () => {
    const original = Array.from({ length: 240 }, (_, i) => "original " + i).join("\n");
    const generated = original.replace("original 130", "reviewed 130");
    const a = previewKnowledgeOrganizationDiff(original, generated);
    const b = previewKnowledgeOrganizationDiff(original, generated);
    expect(a).toEqual(b);
    expect(a.truncated).toBe(false);
    expect(a.added).toBe(1);
    expect(a.removed).toBe(1);
    expect(a.rows.length).toBe(241);
  });

  it("makes the bounded inline diff an explicit pre-confirmation preview only", () => {
    expect(page).toContain("previewKnowledgeOrganizationDiff(data.content, organizeDraft)");
    expect(page).toContain("整理差异预览（仅供人工核对）");
    expect(page).toContain("organizationDiff.truncated");
    expect(page).toContain("差异预览已截断");
    expect(page).toContain("原知识不会被修改");
    expect(page).toContain("organizationDiff.rows.map((row, index) =>");
    expect(page).toContain("organizeConfirmed");
    expect(page).toContain("createOrganizedKnowledge");
  });
});
