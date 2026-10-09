import { describe, expect, it } from "vitest";
import { MockAiksApi } from "./mock";

describe("project memory workbench", () => {
  it("provides a scoped snapshot and creates reviewable read-only outputs", async () => {
    const api = new MockAiksApi();
    const projects = await api.getProjectMemories();
    expect(projects.length).toBeGreaterThan(0);
    const first = projects[0];
    expect(first.id).toBeTruthy();
    expect(first.session_count).toBeGreaterThan(0);
    const snapshot = await api.getProjectMemory(first.id);
    expect(snapshot.project.id).toBe(first.id);
    const report = await api.createProjectReview(first.id, "2020-01-01", "2030-12-31");
    expect(report).toContain(first.title);
    const context = await api.createAgentContextPack(first.id, 1000);
    expect(context).toContain(first.title);
    await expect(api.createProjectReview(first.id, "2030-01-01", "2020-01-01"))
      .rejects.toThrow();
  });
});
