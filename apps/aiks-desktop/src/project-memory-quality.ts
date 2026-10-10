import type { ProjectMemoryEntry } from "./api/types";

export interface ProjectMemoryQualitySummary {
  incorrect: number;
  outdated: number;
  needsDetail: number;
  duplicate: number;
}

/** Only explicit user feedback is reported; this does not infer factual contradictions. */
export function summarizeProjectMemoryQuality(entries: ProjectMemoryEntry[]): ProjectMemoryQualitySummary {
  const result: ProjectMemoryQualitySummary = { incorrect: 0, outdated: 0, needsDetail: 0, duplicate: 0 };
  for (const entry of entries) {
    switch (entry.feedback_status) {
      case "incorrect": result.incorrect += 1; break;
      case "outdated": result.outdated += 1; break;
      case "needs_detail": result.needsDetail += 1; break;
      case "duplicate": result.duplicate += 1; break;
    }
  }
  return result;
}
