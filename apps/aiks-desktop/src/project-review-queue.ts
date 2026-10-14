import type {
  ProjectEvolutionSnapshot, ProjectMemoryEntry, ProjectMemorySnapshot,
} from "./api/types";

/**
 * S3.1 project review inbox. Every item comes from a real human feedback
 * event or an existing, source-verified relation. Nothing here is inferred
 * from similar wording, matching titles, or an LLM's supposed certainty.
 */
export type ProjectReviewSignal = "feedback" | "relation";

export interface ProjectReviewCandidate {
  id: string;
  kind: ProjectReviewSignal;
  // Useful / incorrect are user statements, not objective truth labels.
  title: string;
  reason: string;
  knowledgeIds: string[];
  sourceSessionIds: number[];
  relationId: string | null;
  relationStatus: "suggested" | "confirmed" | null;
  evidence: string;
  updatedAt: string;
}

export interface ProjectReviewQueue {
  candidates: ProjectReviewCandidate[];
  truncated: boolean;
  feedbackCount: number;
  relationCount: number;
}

const REVIEW_FEEDBACK: Readonly<Record<string, string>> = {
  incorrect: "用户标记：可能有错误",
  outdated: "用户标记：可能已过时",
  needs_detail: "用户标记：需要补充上下文",
  duplicate: "用户标记：可能重复",
};

const RELATION_REASON: Readonly<Record<string, string>> = {
  corrects: "存在修正关系，需复核两份证据与适用范围",
  supersedes: "存在替代关系，需复核旧方案是否仍适用",
  resolved_by: "存在解决关系，需复核问题是否实际解决",
};

/** Called only when BOTH projections have loaded from the same selected project. */
export function buildProjectReviewQueue(
  memory: ProjectMemorySnapshot,
  evolution: ProjectEvolutionSnapshot,
): ProjectReviewQueue {
  const candidates: ProjectReviewCandidate[] = [];
  const byId = new Map<string, ProjectMemoryEntry>(
    memory.entries.map(entry => [entry.knowledge_id, entry]),
  );
  let feedbackCount = 0;
  let relationCount = 0;

  for (const entry of memory.entries) {
    const reason = entry.feedback_status
      ? REVIEW_FEEDBACK[entry.feedback_status]
      : undefined;
    if (!reason) continue;
    feedbackCount += 1;
    candidates.push({
      id: "feedback:" + entry.knowledge_id,
      kind: "feedback",
      title: entry.title,
      reason,
      knowledgeIds: [entry.knowledge_id],
      sourceSessionIds: [entry.session_id],
      relationId: null,
      relationStatus: null,
      evidence: "该条目基于用户明确提交的质量反馈，不代表 AIKS 已独立核实。",
      updatedAt: entry.updated_at,
    });
  }

  for (const link of evolution.links) {
    const reason = RELATION_REASON[link.relation_type];
    if (!reason || (link.status !== "suggested" && link.status !== "confirmed")) continue;
    if (link.source_id === link.target_id) continue;
    relationCount += 1;
    const endpoints = [byId.get(link.source_id), byId.get(link.target_id)]
      .filter((item): item is ProjectMemoryEntry => item !== undefined);
    candidates.push({
      id: "relation:" + link.id,
      kind: "relation",
      title: link.source_title + " → " + link.target_title,
      reason: link.status === "suggested"
        ? "待人工核对：" + reason
        : "已有人确认关联：" + reason + "（不等于事实已被证明）",
      knowledgeIds: [link.source_id, link.target_id],
      sourceSessionIds: [...new Set(endpoints.map(item => item.session_id))],
      relationId: link.id,
      relationStatus: link.status,
      evidence: link.evidence,
      updatedAt: link.updated_at,
    });
  }

  // Do not mutate either source projection or discard different evidence
  // supporting the same knowledge. IDs and time are stable tie breakers.
  candidates.sort((a, b) => b.updatedAt.localeCompare(a.updatedAt) || a.id.localeCompare(b.id));
  return {
    candidates: candidates.slice(0, 100),
    feedbackCount,
    relationCount,
    truncated: memory.truncated || evolution.truncated || candidates.length > 100,
  };
}
