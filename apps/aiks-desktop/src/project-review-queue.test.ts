import { describe, expect, it } from "vitest";
import type {
  ProjectEvolutionLink, ProjectEvolutionSnapshot, ProjectMemoryEntry,
  ProjectMemorySnapshot,
} from "./api/types";
import { buildProjectReviewQueue } from "./project-review-queue";
import page from "./pages/ProjectMemoryPage.tsx?raw";

function knowledge(id: string, feedback: string | null, updatedAt = "2026-10-11T09:00:00Z"): ProjectMemoryEntry {
  return {
    knowledge_id: id, session_id: id.length, source: "codex", session_external_id: "session-" + id,
    title: "Knowledge " + id, category: "decision", summary: "This is not a verified fact",
    updated_at: updatedAt, feedback_status: feedback,
  };
}

function link(id: string, status: "confirmed" | "suggested", type: ProjectEvolutionLink["relation_type"] = "corrects"): ProjectEvolutionLink {
  return {
    id, source_id: "k1", target_id: "k2", source_title: "Source", target_title: "Target",
    relation_type: type, status, evidence: "user-supplied Session reference",
    updated_at: "2026-10-11T10:00:00Z",
  };
}

function pair(
  entries: ProjectMemoryEntry[],
  links: ProjectEvolutionLink[],
  truncated = false,
): [ProjectMemorySnapshot, ProjectEvolutionSnapshot] {
  return [
    {
      project: {
        id: "project:test", title: "Test", verified_path: true, session_count: 2,
        knowledge_count: entries.length, sources: ["codex"], last_updated_at: "2026-10-11",
      },
      entries, truncated,
    },
    { links, truncated: false },
  ];
}

describe("S3.1 human-evidence project quality review inbox", () => {
  it("only flags explicit user feedback and records relationship status without inventing facts", () => {
    const [memory, evolution] = pair(
      [knowledge("k1", "incorrect"), knowledge("k2", "useful"), knowledge("k3", null),
       knowledge("k4", "outdated"), knowledge("k5", "needs_detail"), knowledge("k6", "duplicate")],
      [link("relation-confirmed", "confirmed", "supersedes"),
       link("relation-pending", "suggested", "resolved_by")],
    );
    const original = JSON.stringify([memory, evolution]);
    const queue = buildProjectReviewQueue(memory, evolution);
    expect(queue.feedbackCount).toBe(4);
    expect(queue.relationCount).toBe(2);
    expect(queue.candidates).toHaveLength(6);
    expect(queue.candidates.filter(item => item.kind === "feedback")
      .map(item => item.knowledgeIds[0]).sort()).toEqual(["k1", "k4", "k5", "k6"]);
    expect(queue.candidates.find(item => item.id === "relation:relation-confirmed")
      ?.reason).toContain("不等于事实已被证明");
    expect(queue.candidates.find(item => item.id === "relation:relation-pending")
      ?.reason).toContain("待人工核对");
    expect(queue.candidates.find(item => item.id === "relation:relation-confirmed")
      ?.knowledgeIds).toEqual(["k1", "k2"]);
    expect(JSON.stringify([memory, evolution])).toBe(original);
  });

  it("does not create contradiction candidates for unrelated evidence or empty relations", () => {
    const [memory, evolution] = pair(
      [knowledge("k1", null), knowledge("k2", "useful")], [],
    );
    expect(buildProjectReviewQueue(memory, evolution)).toEqual({
      candidates: [], truncated: false, feedbackCount: 0, relationCount: 0,
    });
  });

  it("uses stable ordering and only promises a bounded window", () => {
    const rows = Array.from({ length: 120 }, (_, index) =>
      knowledge("k-" + index, "outdated", "2026-10-11T09:00:00Z"),
    );
    const [memory, evolution] = pair(rows, [link("suggested", "suggested")]);
    const queue = buildProjectReviewQueue(memory, evolution);
    expect(queue.candidates).toHaveLength(100);
    expect(queue.feedbackCount).toBe(120);
    expect(queue.relationCount).toBe(1);
    expect(queue.truncated).toBe(true);
    expect(queue.candidates[0].id).toBe("relation:suggested");
    const [limitedMemory, limitedEvolution] = pair([knowledge("a", null)], [], true);
    expect(buildProjectReviewQueue(limitedMemory, limitedEvolution).truncated).toBe(true);
  });

  it("shows linked, source-verifiable review cues with no autonomous editing", () => {
    expect(page).toContain("buildProjectReviewQueue(detail, evolution)");
    expect(page).toContain("evolutionProjectId === selected");
    expect(page).toContain("项目知识复核队列（明确证据）");
    expect(page).toContain("模型摘要自动推断矛盾");
    expect(page).toContain("item.knowledgeIds.map(id =>");
    expect(page).toContain("item.sourceSessionIds.map(id =>");
    expect(page).toContain('reviewEvolution(item.relationId!, "confirmed")');
    expect(page).toContain("reviewQueue.truncated");
    expect(page).not.toContain("autoArchiveContradictions");
  });
});
