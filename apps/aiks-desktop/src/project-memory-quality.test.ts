import { describe, expect, it } from "vitest";
import type { ProjectMemoryEntry } from "./api/types";
import { summarizeProjectMemoryQuality } from "./project-memory-quality";

const entry = (status: string | null): ProjectMemoryEntry => ({
  knowledge_id: "k", session_id: 1, source: "codex", session_external_id: "s",
  title: "Knowledge", category: "decision", summary: "Evidence",
  updated_at: "2026-10-10", feedback_status: status,
});

describe("project memory feedback risk summary", () => {
  it("counts only explicitly reported quality concerns", () => {
    const result = summarizeProjectMemoryQuality([
      entry("incorrect"), entry("outdated"), entry("needs_detail"),
      entry("duplicate"), entry("useful"), entry(null), entry("outdated"),
    ]);
    expect(result).toEqual({ incorrect: 1, outdated: 2, needsDetail: 1, duplicate: 1 });
  });

  it("does not invent risks for unreviewed entries", () => {
    expect(summarizeProjectMemoryQuality([entry(null), entry("useful")]))
      .toEqual({ incorrect: 0, outdated: 0, needsDetail: 0, duplicate: 0 });
  });
});
