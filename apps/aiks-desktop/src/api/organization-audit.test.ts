import { describe, expect, it } from "vitest";
import { MockAiksApi } from "./mock";
import { isReviewedOrganizationOperation } from "./ai-assist";
import page from "../pages/KnowledgeDetailPage.tsx?raw";
import tauri from "./tauri.ts?raw";
import rustCommand from "../../src-tauri/src/knowledge_commands.rs?raw";
import migration from "../../../../crates/aiks-core/migrations/015_organization_audit.sql?raw";

describe("S3.4 durable reviewed organization audit", () => {
  it("records an independent copy with ordered source IDs and restores state across API instances", async () => {
    const api = new MockAiksApi();
    const source = "kn_0001";
    const other = "kn_0005";
    const title = "S3.4 audited reviewed comparison";
    const created = await api.createOrganizedKnowledge({
      title, content: "User reviewed Markdown", tags: [],
      project_name: "Pipeline",
    }, [source, other], "compare");
    const reopened = new MockAiksApi();
    const entry = (await reopened.getOrganizationAudit(source)).find(
      item => item.derived_knowledge_id === created.id,
    );
    expect(entry).toMatchObject({
      derived_knowledge_id: created.id,
      operation: "compare",
      title,
      status: "active",
      source_knowledge_ids: [source, other],
    });
    expect((await reopened.getOrganizationAudit(other)).some(
      item => item.derived_knowledge_id === created.id,
    )).toBe(true);
    await reopened.archiveKnowledge(created.id);
    expect((await api.getOrganizationAudit(source)).find(
      item => item.derived_knowledge_id === created.id,
    )?.status).toBe("archived");
    await api.restoreKnowledge(created.id);
    expect((await reopened.getOrganizationAudit(source)).find(
      item => item.derived_knowledge_id === created.id,
    )?.status).toBe("active");
    expect((await reopened.getKnowledgeDetail(source)).status).toBe("active");
  });

  it("does not treat ordinary AI assistance operations as reviewed organization", () => {
    expect(isReviewedOrganizationOperation("summary")).toBe(false);
    expect(isReviewedOrganizationOperation("title")).toBe(false);
    expect(isReviewedOrganizationOperation("tags")).toBe(false);
    expect(isReviewedOrganizationOperation("category")).toBe(false);
    for (const operation of ["structure", "rewrite", "key_conclusions", "compare", "merge_draft"] as const) {
      expect(isReviewedOrganizationOperation(operation)).toBe(true);
    }
  });

  it("blocks invalid audit metadata before creating any document", async () => {
    const api = new MockAiksApi();
    const input = { title: "Forbidden audit title", content: "do not create", tags: [] };
    await expect(api.createOrganizedKnowledge(input, ["kn_0001"], "compare")).rejects.toThrow();
    await expect(api.createOrganizedKnowledge(input, ["kn_0001"], "summary")).rejects.toThrow();
    await expect(api.createOrganizedKnowledge(input, [], "structure")).rejects.toThrow();
    expect((await api.getKnowledge({ limit: 200 })).items.some(item =>
      item.title === "Forbidden audit title"
    )).toBe(false);
  });

  it("registers a source-based persisted history, not an ephemeral last-created-ID-only control", () => {
    expect(page).toContain("getApi().getOrganizationAudit(knowledgeId)");
    expect(page).toContain("人工审核的知识整理历史");
    expect(page).toContain("organizationHistory.map(entry =>");
    expect(page).toContain('entry.status === "archived"');
    expect(page).toContain('entry.status === "active"');
    expect(tauri).toContain('invoke("list_organization_audit", { sourceKnowledgeId })');
    expect(rustCommand).toContain("OrganizationAuditRepo::new(&db).record(");
    expect(rustCommand).toContain("请勿重复点击创建");
    expect(migration).toContain("CREATE TABLE IF NOT EXISTS knowledge_organization_record");
    expect(migration).toContain("CREATE TABLE IF NOT EXISTS knowledge_organization_source");
  });
});
