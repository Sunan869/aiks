import type { KnowledgeRelation } from "./api/types";

export function visibleEvolutionTimeline(relations: KnowledgeRelation[]): KnowledgeRelation[] {
  // Only human-confirmed evidence belongs on the authoritative timeline.
  // Keep rejected and pending suggestions in the separate review controls.
  return relations.filter(item => item.status === "confirmed")
    .sort((a, b) => a.updated_at.localeCompare(b.updated_at) || a.id.localeCompare(b.id));
}
