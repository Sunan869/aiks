import { describe, expect, it } from "vitest";
import { MockAiksApi } from "./mock";
import knowledgePage from "../pages/KnowledgeDetailPage.tsx?raw";
import relationRust from "../../../../crates/aiks-core/src/pipeline/relation_repo.rs?raw";

describe("S2.3 source-verified human-reviewed cross-session relations", () => {
  it("uses same canonical verified project and refuses identical-looking different projects", async () => {
    const api = new MockAiksApi();
    // Provider Sessions 1/5 are both /projects/pipeline; session 2 is another directory.
    const compatible = await api.suggestKnowledgeRelation(
      "kn_0001", "kn_0005", "supplements", "same canonical project; reviewed separately",
    );
    expect(compatible.status).toBe("suggested");
    expect((await api.reviewKnowledgeRelation(compatible.id, "confirmed")).status).toBe("confirmed");
    await expect(api.suggestKnowledgeRelation(
      "kn_0001", "kn_0002", "related", "display names do not prove provenance",
    )).rejects.toThrow();
    await expect(api.suggestKnowledgeRelation(
      "kn_0001", "kn_0004", "related", "manual knowledge needs project verification",
    )).rejects.toThrow();
  });

  it("allows explicit same-project manual annotations but not unverified mixed-source links", async () => {
    const api = new MockAiksApi();
    const first = await api.createKnowledge({
      title: "Manual relation source", content: "Original", tags: [], project_name: "manual-team",
    });
    const second = await api.createKnowledge({
      title: "Manual relation target", content: "Reviewed", tags: [], project_name: "manual-team",
    });
    const incompatible = await api.createKnowledge({
      title: "Another user project", content: "Not connected", tags: [], project_name: "manual-unrelated",
    });
    const relation = await api.suggestKnowledgeRelation(
      first.id, second.id, "corrects", "User compared the two manual documents",
    );
    expect((await api.reviewKnowledgeRelation(relation.id, "confirmed")).status).toBe("confirmed");
    await expect(api.suggestKnowledgeRelation(
      first.id, incompatible.id, "related", "not the same manually scoped project",
    )).rejects.toThrow();
    await expect(api.suggestKnowledgeRelation(
      first.id, "kn_0001", "related", "one side is a Provider Session",
    )).rejects.toThrow();
  });

  it("revalidates both Knowledge endpoints before human confirmation, but keeps rejection possible", async () => {
    const api = new MockAiksApi();
    const link = await api.suggestKnowledgeRelation(
      "kn_0001", "kn_0005", "resolved_by", "Human inspection required",
    );
    await api.archiveKnowledge("kn_0005");
    try {
      await expect(api.reviewKnowledgeRelation(link.id, "confirmed")).rejects.toThrow();
      expect((await api.getKnowledgeRelations("kn_0001")).find(item => item.id === link.id)?.status)
        .toBe("suggested");
      expect((await api.reviewKnowledgeRelation(link.id, "rejected")).status).toBe("rejected");
    } finally {
      await api.restoreKnowledge("kn_0005");
    }
  });

  it("explains pathless and missing Session isolation at the UI and Rust data boundary", () => {
    expect(relationRust).toContain("Unverified source projects require the same Session");
    expect(relationRust).toContain("Missing source Session cannot establish a knowledge relation");
    expect(relationRust).toContain("project_session_binding pb ON pb.session_id = ss.id");
    expect(relationRust).toContain("missing_source_after_proposal_blocks_confirmation_but_keeps_rejection");
    expect(knowledgePage).toContain("两个不同 Session 仅显示项目同名不能作为证据");
    expect(knowledgePage).toContain("提交建议与人工确认时 Rust 后端会独立核验");
  });
});
