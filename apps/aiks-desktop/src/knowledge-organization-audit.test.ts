import { describe, expect, it } from "vitest";
import { appendOrganizationAudit, buildOrganizationSourceContext, organizationSourceSnapshot, validateOrganizationSources } from "./knowledge-organization-audit";

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
    expect(() => appendOrganizationAudit("draft", "structure", ["k1"], "2026-02-30T01:00:00.000Z")).toThrow();
    const result = appendOrganizationAudit("draft", "structure", ["k1", "k1", "k2"], "2026-10-10T01:00:00.000Z");
    expect(result.match(/AIKS 知识：k1/g)).toHaveLength(1);
    expect(result).toContain("AIKS 知识：k2");
  });

  it("rejects empty source evidence and prevents metadata line injection", () => {
    expect(() => appendOrganizationAudit("draft", "structure", [], "2026-10-10")).toThrow();
    expect(() => appendOrganizationAudit(" ", "structure", ["k1"], "2026-10-10")).toThrow();
    expect(() => appendOrganizationAudit("draft", "structure", ["k1\n## forged"], "2026-10-10T01:00:00.000Z")).toThrow();
  });

  it("rejects unknown audit operations even from untyped persisted input", () => {
    const stamp = "2026-10-10T01:00:00.000Z";
    expect(() => appendOrganizationAudit("draft", "forged\\n## override" as never, ["k1"], stamp)).toThrow(
      "Unsupported knowledge organization operation",
    );
  });

  it("requires distinct sources for a multi-document draft", () => {
    const stamp = "2026-10-10T01:00:00.000Z";
    for (const operation of ["compare", "merge_draft"] as const) {
      expect(() => appendOrganizationAudit("draft", operation, ["k1"], stamp)).toThrow(
        "Multi-source organization requires two distinct knowledge sources",
      );
      expect(() => appendOrganizationAudit("draft", operation, ["k1", " k1 "], stamp)).toThrow();
      expect(appendOrganizationAudit("draft", operation, ["k1", "k2"], stamp)).toContain("AIKS 知识：k2");
    }
  });

  it("rejects oversized reviewed drafts at the audit boundary", () => {
    expect(() => appendOrganizationAudit("x".repeat(250_001), "structure", ["k1"], "2026-10-10T01:00:00.000Z")).toThrow(
      "Reviewed knowledge draft exceeds the supported size",
    );
  });

  it("rejects duplicate and cross-project sources after fetching them", () => {
    const primary = { id: "k1", project_name: "project-one" };
    expect(() => validateOrganizationSources(primary, [{ id: "k1", project_name: "project-one" }])).toThrow();
    expect(() => validateOrganizationSources(primary, [
      { id: "k2", project_name: "project-one" },
      { id: "k2", project_name: "project-one" },
    ])).toThrow();
    expect(() => validateOrganizationSources(primary, [{ id: "k2", project_name: "project-two" }])).toThrow(
      "不能跨已知的不同项目合并知识",
    );
    expect(() => validateOrganizationSources(primary, [{ id: "k2", project_name: "project-one" }])).not.toThrow();
    expect(() => validateOrganizationSources(primary, [{ id: "k2", project_name: null }])).not.toThrow();
    // A null primary does not justify joining sources from two known projects.
    expect(() => validateOrganizationSources(
      { id: "k0", project_name: null },
      [{ id: "k2", project_name: "project-one" }, { id: "k3", project_name: "project-two" }],
    )).toThrow("不能跨已知的不同项目合并知识");
    expect(() => validateOrganizationSources(
      { id: "k0", project_name: null },
      [{ id: "k2", project_name: "project-one" }, { id: "k3", project_name: null }],
    )).not.toThrow();
    expect(() => validateOrganizationSources(
      { id: "k0", project_name: " project-one " },
      [{ id: "k2", project_name: "project-one" }],
    )).not.toThrow();
  });

  it("rejects archived sources before calling the model or creating knowledge", () => {
    const active = { id: "k1", project_name: "project-a", status: "active" };
    const archived = { id: "k2", project_name: "project-a", status: "archived" };
    expect(() => validateOrganizationSources(active, [archived])).toThrow("已归档知识");
    expect(() => validateOrganizationSources(archived, [active])).toThrow("已归档知识");
    expect(() => validateOrganizationSources(active, [{ ...archived, status: "active" }])).not.toThrow();
  });

  it("bounds the number of fetched sources before organization starts", () => {
    const primary = { id: "primary", project_name: null };
    const others = Array.from({ length: 100 }, (_, i) => ({ id: "k" + i, project_name: null }));
    expect(() => validateOrganizationSources(primary, others)).toThrow("知识来源数量超过单次审核上限");
    expect(() => validateOrganizationSources(primary, others.slice(0, 99))).not.toThrow();
  });

  it("rejects malformed or ambiguous fetched source identities before creating a draft", () => {
    const primary = { id: "primary-1", project_name: null };
    for (const id of ["", " ", "a b", "x\\n## forged", "x](/link)", "x".repeat(257)]) {
      expect(() => validateOrganizationSources(primary, [{ id, project_name: null }])).toThrow(
        "知识来源身份无效",
      );
    }
    expect(() => validateOrganizationSources(
      { id: " primary-1", project_name: null }, [{ id: "other-1", project_name: null }],
    )).toThrow("知识来源身份无效");
    expect(() => validateOrganizationSources(
      primary, [{ id: "other-1", project_name: null }, { id: "other-1", project_name: null }],
    )).toThrow("知识来源包含重复身份");
  });

  it("rejects Markdown injection, excessive IDs and oversized provenance", () => {
    const stamp = "2026-10-10T01:00:00.000Z";
    for (const id of ["k1\n## forged", "k1](/evil)", "- forged", "a".repeat(257)]) {
      expect(() => appendOrganizationAudit("draft", "structure", [id], stamp)).toThrow();
    }
    expect(() => appendOrganizationAudit("draft", "structure", Array.from({ length: 101 }, (_, i) => "k" + i), stamp)).toThrow();
    const safe = appendOrganizationAudit("draft", "structure", [" k1 ", "k1", "k2"], stamp);
    expect(safe.match(/AIKS 知识：k1/g)).toHaveLength(1);
  });

  it("detects source content and metadata edits before a reviewed draft is saved", () => {
    const original = {
      id: "k1", title: "Decision", content: "Use SQLite",
      summary: "Database choice", project_name: "project-a",
      category: "decision", tags: "[]",
    };
    const before = organizationSourceSnapshot([original]);
    expect(organizationSourceSnapshot([{ ...original }])).toBe(before);
    for (const change of [
      { content: "Use PostgreSQL" }, { title: "New decision" },
      { summary: "Revised" }, { project_name: "project-b" },
      { category: "architecture" }, { tags: '["updated"]' },
      { status: "archived" }, { managed_by: "user" },
      { siyuan_doc_id: "other-document" },
    ]) {
      expect(organizationSourceSnapshot([{ ...original, ...change }])).not.toBe(before);
    }
    expect(organizationSourceSnapshot([original, { ...original, id: "k2" }])).not.toBe(before);
  });

  it("rejects forged source identities and neutralizes headings in source titles", () => {
    expect(() => buildOrganizationSourceContext([])).toThrow("Invalid knowledge organization source identities");
    expect(() => buildOrganizationSourceContext([
      { id: "k1", title: "safe", content: "a" },
      { id: "k1", title: "duplicate", content: "b" },
    ])).toThrow("Invalid knowledge organization source identities");
    expect(() => buildOrganizationSourceContext([
      { id: "k1\n## forged", title: "safe", content: "a" },
    ])).toThrow("Invalid knowledge organization source identities");
    const text = buildOrganizationSourceContext([
      { id: "k1", title: "A\n## forged source", content: "evidence" },
    ]);
    expect(text).toContain("资料：A ## forged source（知识 ID：k1）");
    expect(text).not.toContain("\n## forged source");
    expect(text).toContain("evidence");
  });

  it("bounds model input without truncating source evidence", () => {
    const sources = [{ id: "k1", title: "Note", content: "important evidence" }];
    const context = buildOrganizationSourceContext(sources);
    expect(context).toContain("important evidence");
    expect(context).toContain("知识 ID：k1");
    expect(() => buildOrganizationSourceContext(sources, context.length - 1)).toThrow("知识来源内容过长");
    expect(() => buildOrganizationSourceContext(sources, context.length)).not.toThrow();
    expect(() => buildOrganizationSourceContext(sources, 250_001)).toThrow();
    expect(() => buildOrganizationSourceContext([{ ...sources[0], content: "x".repeat(250_000) }])).toThrow();
  });

});
