import { describe, expect, it } from "vitest";
import { MockAiksApi } from "./mock";

describe("controlled knowledge organization", () => {
  it("generates a draft, creates a separate document, and archives the output without changing the source", async () => {
    const api = new MockAiksApi();
    const first = (await api.getKnowledge({limit: 1})).items[0];
    const original = await api.getKnowledgeDetail(first.id);
    const suggestion = await api.assistKnowledge({
      siyuanDocId: original.siyuan_doc_id || "local-test",
      operation: "merge_draft",
      title: original.title,
      content: "Source: " + original.id + "\n" + original.content,
      existingSummary: original.summary,
    });
    expect(suggestion.text).toContain("合并");
    const created = await api.createKnowledge({
      title: original.title + " · 整理草稿",
      category: original.category,
      project_name: original.project_name,
      summary: "用户审核过的整理文档",
      tags: [],
      content: (suggestion.text || "") + "\n来源：" + original.id,
    });
    expect(created.id).not.toBe(original.id);
    expect((await api.getKnowledgeDetail(original.id)).content).toBe(original.content);
    const archived = await api.archiveKnowledge(created.id);
    expect(archived.status).toBe("archived");
    expect((await api.getKnowledgeDetail(original.id)).status).toBe(original.status);
    const restored = await api.restoreKnowledge(created.id);
    expect(restored.id).toBe(created.id);
    expect(restored.status).not.toBe("archived");
    expect((await api.getKnowledgeDetail(original.id)).content).toBe(original.content);
    expect((await api.getKnowledgeDetail(original.id)).status).toBe(original.status);
  });
});
