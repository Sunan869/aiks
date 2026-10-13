import type { ProjectMemoryEntry } from "./api/types";
import { reviewRequiredProjectKnowledge } from "./project-memory-quality";

/**
 * These are display buckets for existing, explicit knowledge categories.
 * Never infer decisions, completed work, or unresolved tasks from free text.
 */
export const PROJECT_MEMORY_TOPICS = [
  { id: "decisions", title: "架构与决策", categories: ["decision", "architecture"] },
  { id: "configuration", title: "环境与配置", categories: ["configuration", "tech_stack", "environment"] },
  { id: "implementation", title: "实施记录", categories: ["implementation", "verification", "testing"] },
  { id: "troubleshooting", title: "问题与解决", categories: ["troubleshooting", "solution", "bugfix"] },
  { id: "research", title: "研究与探索", categories: ["research"] },
  { id: "open", title: "明确标记的约束和待办", categories: ["constraint", "risk", "todo", "task"] },
  { id: "other", title: "其他来源记录", categories: [] },
] as const;

export type ProjectMemoryTopic = (typeof PROJECT_MEMORY_TOPICS)[number]["id"];
export type ProjectMemoryTopicFilter = ProjectMemoryTopic | "all";

export interface ProjectMemoryViewOptions {
  topic: ProjectMemoryTopicFilter;
  feedbackOnly: boolean;
  source?: string;
}

export interface ProjectMemoryTimelineDay {
  day: string;
  entries: ProjectMemoryEntry[];
}

export function projectMemoryTopic(category: string): ProjectMemoryTopic {
  const normalized = category.trim().toLowerCase();
  for (const topic of PROJECT_MEMORY_TOPICS) {
    if (topic.id !== "other" && (topic.categories as readonly string[]).includes(normalized)) {
      return topic.id;
    }
  }
  return "other";
}

/** Category counts are strictly from the currently loaded bounded snapshot. */
export function countProjectMemoryTopics(entries: readonly ProjectMemoryEntry[]): Record<ProjectMemoryTopic, number> {
  const result: Record<ProjectMemoryTopic, number> = {
    decisions: 0, configuration: 0, implementation: 0,
    troubleshooting: 0, research: 0, open: 0, other: 0,
  };
  for (const entry of entries) result[projectMemoryTopic(entry.category)] += 1;
  return result;
}

/** Return references to evidence, without mutating or rewriting canonical rows. */
export function selectProjectMemoryEntries(
  entries: readonly ProjectMemoryEntry[],
  options: ProjectMemoryViewOptions,
): ProjectMemoryEntry[] {
  const flagged = options.feedbackOnly
    ? new Set(reviewRequiredProjectKnowledge([...entries]).map(entry => entry.knowledge_id))
    : null;
  return entries.filter(entry =>
    (options.topic === "all" || projectMemoryTopic(entry.category) === options.topic)
    && (!flagged || flagged.has(entry.knowledge_id))
    && (!options.source || entry.source === options.source)
  );
}

export function recordedUpdateDay(timestamp: string): string | null {
  const day = timestamp.slice(0, 10);
  if (!/^\d{4}-\d{2}-\d{2}$/.test(day)) return null;
  const date = new Date(day + "T00:00:00Z");
  return Number.isNaN(date.getTime()) || date.toISOString().slice(0, 10) !== day
    ? null
    : day;
}

/**
 * Timeline represents knowledge UPDATED dates, not historical event dates.
 * Ties use stable knowledge identity so repeated loads have stable output.
 */
export function groupProjectMemoryUpdates(entries: readonly ProjectMemoryEntry[]): ProjectMemoryTimelineDay[] {
  const rows = entries.map(entry => ({ entry, day: recordedUpdateDay(entry.updated_at) }));
  rows.sort((a, b) =>
    (b.day || "").localeCompare(a.day || "")
    || b.entry.updated_at.localeCompare(a.entry.updated_at)
    || a.entry.knowledge_id.localeCompare(b.entry.knowledge_id)
  );
  const groups: ProjectMemoryTimelineDay[] = [];
  for (const row of rows) {
    const day = row.day || "时间未记录";
    if (groups[groups.length - 1]?.day !== day) groups.push({ day, entries: [] });
    groups[groups.length - 1].entries.push(row.entry);
  }
  return groups;
}
