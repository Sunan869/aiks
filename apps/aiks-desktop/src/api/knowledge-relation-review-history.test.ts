import { describe, expect, it } from "vitest";
import { MockAiksApi } from "./mock";
import detailPage from "../pages/KnowledgeDetailPage.tsx?raw";
import rustRepo from "../../../../crates/aiks-core/src/pipeline/relation_repo.rs?raw";
import command from "../../src-tauri/src/knowledge_commands.rs?raw";
import tauri from "./tauri.ts?raw";

describe("S2.3 human relationship review audit history", () => {
  it("records decisions only when status changes, preserves rejection, and survives reload", async () => {
    const api = new MockAiksApi();
    const left = await api.createKnowledge({
      title: "Audit source", content: "Original text", tags: [], project_name: "relation-audit-project",
    });
    const right = await api.createKnowledge({
      title: "Audit target", content: "No edits to source", tags: [], project_name: "relation-audit-project",
    });
    const link = await api.suggestKnowledgeRelation(left.id, right.id, "corrects", "Explicit manual evidence");
    expect((await api.getKnowledgeRelationReviews(left.id)).reviews).toEqual([]);
    expect((await api.reviewKnowledgeRelation(link.id, "confirmed")).status).toBe("confirmed");
    expect((await api.reviewKnowledgeRelation(link.id, "rejected")).status).toBe("rejected");
    await api.reviewKnowledgeRelation(link.id, "rejected");
    const afterReopen = await new MockAiksApi().getKnowledgeRelationReviews(right.id);
    expect(afterReopen.reviews).toHaveLength(2);
    expect(afterReopen.truncated).toBe(false);
    expect(afterReopen.reviews.map(item => item.next_status).sort()).toEqual(["confirmed", "rejected"]);
    expect(afterReopen.reviews.every(item =>
      item.source_id === left.id && item.target_id === right.id
    )).toBe(true);
    expect(afterReopen.reviews.some(item =>
      item.previous_status === "suggested" && item.next_status === "confirmed"
    )).toBe(true);
    expect((await api.getKnowledgeDetail(left.id)).content).toBe("Original text");
    expect((await api.getKnowledgeDetail(right.id)).content).toBe("No edits to source");
  });

  it("shows the immutable decision trail separately from the authoritative evolution timeline", () => {
    expect(detailPage).toContain("getApi().getKnowledgeRelationReviews(knowledgeId)");
    expect(detailPage).toContain("relationReviewHistory?.reviews.map(review =>");
    expect(detailPage).toContain("人工审核历史（本地持久记录）");
    expect(detailPage).toContain("审核时间不等于知识生效时间");
    expect(detailPage).toContain("setRelationReviewHistoryRevision(value => value + 1)");
    expect(detailPage).toContain("relationReviewHistory?.truncated");
    expect(tauri).toContain('invoke("list_knowledge_relation_reviews", { knowledgeId })');
    expect(command).toContain("pub async fn list_knowledge_relation_reviews(");
    expect(rustRepo).toContain("pub fn review_history(");
    expect(rustRepo).toContain("FROM knowledge_relation_review rr");
    expect(rustRepo).toContain("review_history_is_durable_ordered_bounded_and_includes_rejected_reviews");
  });
});
