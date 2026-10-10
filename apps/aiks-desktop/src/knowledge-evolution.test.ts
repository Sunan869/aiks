import { describe, expect, it } from "vitest";
import type { KnowledgeRelation } from "./api/types";
import { visibleEvolutionTimeline } from "./knowledge-evolution";

const make = (id: string, status: KnowledgeRelation["status"], updated_at: string): KnowledgeRelation => ({
  id, status, updated_at, source_id: "source", target_id: "target",
  relation_type: "corrects", evidence: "session proof",
  confidence: null, created_at: updated_at,
});

describe("confirmed knowledge evolution timeline", () => {
  it("sorts confirmed relationships consistently without mutating input", () => {
    const entries = [
      make("b", "confirmed", "2026-10-10"),
      make("pending", "suggested", "2026-10-08"),
      make("c", "confirmed", "2026-10-10"),
      make("rejected", "rejected", "2026-10-09"),
      make("a", "confirmed", "2026-10-09"),
    ];
    expect(visibleEvolutionTimeline(entries).map(item => item.id)).toEqual(["a", "b", "c"]);
    expect(entries[0].id).toBe("b");
    expect(visibleEvolutionTimeline(entries.filter(item => item.status !== "confirmed"))).toEqual([]);
  });
});
