import { describe, expect, it } from "vitest";
import { MockAiksApi } from "./mock";

describe("knowledge feedback", () => {
  it("keeps feedback history separate from editable knowledge content", async () => {
    const api = new MockAiksApi();
    const page = await api.getKnowledge({ limit: 1 });
    const item = page.items[0];
    expect(item).toBeDefined();
    const original = await api.getKnowledgeDetail(item.id);
    const before = await api.getKnowledgeFeedback(item.id);
    const added = await api.addKnowledgeFeedback(item.id, "incorrect", "The cited cause is incomplete");
    expect(added.kind).toBe("incorrect");
    const after = await api.getKnowledgeFeedback(item.id);
    expect(after).toHaveLength(before.length + 1);
    expect(after[0]?.id).toBe(added.id);
    expect((await api.getKnowledgeDetail(item.id)).content).toBe(original.content);
  });

  it("does not accept feedback for an unknown knowledge identity", async () => {
    const api = new MockAiksApi();
    await expect(api.addKnowledgeFeedback("nonexistent-id", "useful", "")).rejects.toThrow();
  });
});
