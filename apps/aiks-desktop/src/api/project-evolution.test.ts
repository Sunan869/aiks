import { describe, expect, it } from "vitest";
import { MockAiksApi } from "./mock";
import pageSource from "../pages/ProjectMemoryPage.tsx?raw";

describe("S3.1 evidence-linked project evolution", () => {
  it("shows confirmed and proposed links distinctly and excludes rejected reviews", async () => {
    const api = new MockAiksApi();
    const projects = await api.getProjectMemories();
    const selected = await Promise.all(projects.map(async project => ({
      id: project.id,
      entries: (await api.getProjectMemory(project.id)).entries,
    })));
    const project = selected.find(item => item.entries.filter(entry => entry.session_id > 0).length >= 3);
    expect(project).toBeDefined();
    const entries = project!.entries.filter(entry => entry.session_id > 0);
    const suggested = await api.suggestKnowledgeRelation(
      entries[0].knowledge_id, entries[1].knowledge_id, "corrects", "Human review required",
    );
    const reviewed = await api.suggestKnowledgeRelation(
      entries[1].knowledge_id, entries[2].knowledge_id, "supersedes", "Manual comparison",
    );
    await api.reviewKnowledgeRelation(reviewed.id, "confirmed");
    let evolution = await api.getProjectEvolution(project!.id);
    expect(evolution.links.map(link => link.id)).toContain(suggested.id);
    expect(evolution.links.map(link => link.id)).toContain(reviewed.id);
    expect(evolution.links.find(link => link.id === reviewed.id)?.status).toBe("confirmed");
    expect(evolution.links.find(link => link.id === suggested.id)?.status).toBe("suggested");
    await api.reviewKnowledgeRelation(suggested.id, "rejected");
    evolution = await api.getProjectEvolution(project!.id);
    expect(evolution.links.map(link => link.id)).not.toContain(suggested.id);
    await expect(api.getProjectEvolution("unknown-project")).rejects.toThrow();
  });

  it("surfaces provenance and does not infer or auto-accept a contradiction", () => {
    expect(pageSource).toContain('api.getProjectEvolution(selected)');
    expect(pageSource).toContain('待人工审核，不能视为生效结论');
    expect(pageSource).toContain('用户已确认关系');
    expect(pageSource).toContain('link.source_id');
    expect(pageSource).toContain('link.target_id');
    expect(pageSource).toContain('审核证据：{link.evidence}');
    expect(pageSource).toContain('window.confirm(prompt)');
    expect(pageSource).toContain('reviewKnowledgeRelation(relationId, decision)');
    expect(pageSource).toContain('getProjectEvolution(requestProject)');
    expect(pageSource).toContain('requestGeneration !== projectGeneration.current');
    expect(pageSource).toContain('onClick={() => void reviewEvolution(link.id, "confirmed")}');
    expect(pageSource).toContain('onClick={() => void reviewEvolution(link.id, "rejected")}');
    expect(pageSource).toContain('link.status === "suggested"');
    expect(pageSource).not.toContain('autoConfirmKnowledgeRelations(');
  });
});
