import { describe, expect, it } from "vitest";
import { MockAiksApi } from "./mock";

describe("knowledge evolution relations", () => {
  it("requires evidence, keeps suggestions reviewable, and preserves knowledge content", async () => {
    const api = new MockAiksApi();
    const first = await api.createKnowledge({
      title: "Issue and hypothesis", content: "Original evidence", tags: [],
      project_name: "relation-test-project",
    });
    const second = await api.createKnowledge({
      title: "Verified fix", content: "Verified fix content", tags: [],
      project_name: "relation-test-project",
    });
    await expect(api.suggestKnowledgeRelation(first.id, second.id, "corrects", ""))
      .rejects.toThrow();
    const proposed = await api.suggestKnowledgeRelation(
      first.id, second.id, "corrects", "Confirmed in another session"
    );
    expect(proposed.status).toBe("suggested");
    await expect(api.suggestKnowledgeRelation(first.id, second.id, "corrects", "duplicate"))
      .rejects.toThrow();
    const verified = await api.reviewKnowledgeRelation(proposed.id, "confirmed");
    expect(verified.status).toBe("confirmed");
    expect((await api.getKnowledgeRelations(second.id))[0]?.id).toBe(proposed.id);
    expect((await api.getKnowledgeDetail(first.id)).content).toBe("Original evidence");
    expect((await api.getKnowledgeDetail(second.id)).content).toBe("Verified fix content");
  });

  it("blocks known cross-project relationships", async () => {
    const api = new MockAiksApi();
    const a = await api.createKnowledge({
      title: "Alpha", content: "A", tags: [], project_name: "alpha-isolated",
    });
    const b = await api.createKnowledge({
      title: "Beta", content: "B", tags: [], project_name: "beta-isolated",
    });
    await expect(api.suggestKnowledgeRelation(a.id, b.id, "related", "Not the same project"))
      .rejects.toThrow();
  });
});
