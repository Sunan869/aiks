import { describe, expect, it } from "vitest";
import type { ProjectMemoryEntry } from "./api/types";
import {
  PROJECT_MEMORY_TOPICS, countProjectMemoryTopics, groupProjectMemoryUpdates,
  projectMemoryTopic, recordedUpdateDay, selectProjectMemoryEntries,
} from "./project-memory-view";

const entry = (
  id: string,
  category: string,
  day: string,
  source = "codex",
  feedback: string | null = null,
): ProjectMemoryEntry => ({
  knowledge_id: id, session_id: id.length, source, session_external_id: "session-" + id,
  title: id, category, summary: "Evidence only", updated_at: day,
  feedback_status: feedback,
});

describe("project memory deterministic topic and update timeline", () => {
  it("classifies only explicit categories, never infers tasks or decisions from text", () => {
    expect(projectMemoryTopic(" Decision ")).toBe("decisions");
    expect(projectMemoryTopic("architecture")).toBe("decisions");
    expect(projectMemoryTopic("configuration")).toBe("configuration");
    expect(projectMemoryTopic("troubleshooting")).toBe("troubleshooting");
    expect(projectMemoryTopic("risk")).toBe("open");
    expect(projectMemoryTopic("todo")).toBe("open");
    expect(projectMemoryTopic("general")).toBe("other");
    expect(projectMemoryTopic("a decision appears in prose")).toBe("other");
    expect(new Set(PROJECT_MEMORY_TOPICS.map(topic => topic.id)).size).toBe(PROJECT_MEMORY_TOPICS.length);
  });

  it("counts every entry exactly once, including unknown categories", () => {
    const entries = [
      entry("a", "decision", "2026-10-09"), entry("b", "architecture", "2026-10-10"),
      entry("c", "configuration", "2026-10-10"), entry("d", "solution", "2026-10-08"),
      entry("e", "general", "2026-10-09"), entry("f", "unknown", "2026-10-07"),
    ];
    const counts = countProjectMemoryTopics(entries);
    expect(counts).toEqual({
      decisions: 2, configuration: 1, implementation: 0,
      troubleshooting: 1, research: 0, open: 0, other: 2,
    });
    expect(Object.values(counts).reduce((sum, count) => sum + count, 0)).toBe(entries.length);
  });

  it("filters topic, source and user feedback together without changing inputs", () => {
    const inputs = [
      entry("a", "decision", "2026-10-10", "codex", "incorrect"),
      entry("b", "architecture", "2026-10-10", "claude_code", "outdated"),
      entry("c", "decision", "2026-10-09", "codex", "useful"),
      entry("d", "solution", "2026-10-09", "codex", "needs_detail"),
      entry("e", "general", "2026-10-09", "codex", null),
    ];
    const selected = selectProjectMemoryEntries(inputs, {
      topic: "decisions", feedbackOnly: true, source: "codex",
    });
    expect(selected.map(item => item.knowledge_id)).toEqual(["a"]);
    expect(selectProjectMemoryEntries(inputs, { topic: "all", feedbackOnly: true })
      .map(item => item.knowledge_id)).toEqual(["a", "b", "d"]);
    expect(inputs).toHaveLength(5);
    expect(inputs[0].knowledge_id).toBe("a");
  });

  it("groups by update date, not claimed event time, with stable tie breaking", () => {
    const inputs = [
      entry("b", "decision", "2026-10-09T11:10:00Z"),
      entry("older", "research", "2026-10-08T12:00:00Z"),
      entry("a", "decision", "2026-10-09T11:10:00Z"),
      entry("new", "configuration", "2026-10-10T06:00:00Z"),
    ];
    const grouped = groupProjectMemoryUpdates(inputs);
    expect(grouped.map(group => group.day)).toEqual(["2026-10-10", "2026-10-09", "2026-10-08"]);
    expect(grouped[1].entries.map(item => item.knowledge_id)).toEqual(["a", "b"]);
    expect(inputs[0].knowledge_id).toBe("b");
  });

  it("keeps missing or malformed timestamps visible without inventing dates", () => {
    expect(recordedUpdateDay("2026-10-11T09:15:00Z")).toBe("2026-10-11");
    expect(recordedUpdateDay("2026-02-30")).toBeNull();
    expect(recordedUpdateDay("bad")).toBeNull();
    const grouped = groupProjectMemoryUpdates([
      entry("bad", "general", "invalid"), entry("good", "decision", "2026-10-09"),
    ]);
    expect(grouped.map(group => group.day)).toEqual(["2026-10-09", "时间未记录"]);
    expect(grouped[1].entries[0].knowledge_id).toBe("bad");
  });

  it("has a complete empty state", () => {
    expect(groupProjectMemoryUpdates([])).toEqual([]);
    expect(selectProjectMemoryEntries([], { topic: "all", feedbackOnly: false })).toEqual([]);
    expect(Object.values(countProjectMemoryTopics([])).every(count => count === 0)).toBe(true);
  });
});
