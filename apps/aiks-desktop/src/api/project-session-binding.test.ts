import { describe, expect, it } from "vitest";
import { MockAiksApi } from "./mock";
import pageSource from "../pages/ProjectMemoryPage.tsx?raw";
import dbSource from "../../../../crates/aiks-core/src/storage/db.rs?raw";
import coreSource from "../../../../crates/aiks-core/src/knowledge/project_memory.rs?raw";

describe("S3.1 explicit local-only project session bindings", () => {
  it("supports reviewed reassignment and persistent-style undo in the API facade", async () => {
    const api = new MockAiksApi();
    const originalProjects = await api.getProjectMemories();
    const sources = await api.getSessions({ limit: 200 });
    const session = sources.items.find(item => Boolean(item.project_path));
    const target = originalProjects.find(item =>
      item.verified_path && item.id !== session?.project_path
    );
    expect(session).toBeDefined();
    expect(target).toBeDefined();
    const original = originalProjects.find(project => project.id === session!.project_path);
    expect(original).toBeDefined();
    const affectedKnowledge = (await api.getProjectMemory(original!.id)).entries
      .filter(entry => entry.session_id === session!.id).length;
    expect(affectedKnowledge).toBeGreaterThan(0);

    await expect(api.bindProjectSession(session!.id, "project:forged")).rejects.toThrow();
    const saved = await api.bindProjectSession(session!.id, target!.id);
    expect(saved.session_id).toBe(session!.id);
    expect(saved.project_id).toBe(target!.id);
    const after = await api.getProjectMemories();
    expect(after.find(project => project.id === target!.id)?.session_count)
      .toBe(target!.session_count + 1);
    expect(after.find(project => project.id === original!.id)?.session_count)
      .toBe(original!.session_count - 1);
    expect(after.find(project => project.id === target!.id)?.knowledge_count)
      .toBe(target!.knowledge_count + affectedKnowledge);
    expect(after.find(project => project.id === original!.id)?.knowledge_count)
      .toBe(original!.knowledge_count - affectedKnowledge);
    expect((await api.getProjectMemory(target!.id)).entries
      .some(entry => entry.session_id === session!.id)).toBe(true);
    expect((await api.getProjectSessionBindings()).some(binding =>
      binding.session_id === session!.id && binding.project_id === target!.id
    )).toBe(true);
    expect(sources.items.find(item => item.id === session!.id)?.project_path)
      .toBe(session!.project_path);
    expect(await api.clearProjectSessionBinding(session!.id)).toBe(true);
    expect(await api.clearProjectSessionBinding(session!.id)).toBe(false);
    const restored = await api.getProjectMemories();
    expect(restored.find(project => project.id === original!.id)?.session_count)
      .toBe(original!.session_count);
    expect(restored.find(project => project.id === target!.id)?.knowledge_count)
      .toBe(target!.knowledge_count);
    expect(restored.find(project => project.id === original!.id)?.knowledge_count)
      .toBe(original!.knowledge_count);
  });

  it("keeps mutations explicit and protects both canonical Session and original documents", () => {
    expect(pageSource).toContain('label="待调整 Session"');
    expect(pageSource).toContain('label="项目归属目标"');
    expect(pageSource).toContain('window.confirm(message)');
    expect(pageSource).toContain('api.bindProjectSession(sessionId, project.id)');
    expect(pageSource).toContain('api.clearProjectSessionBinding(sessionId)');
    expect(pageSource).toContain('仅更改 AIKS 本地记忆归组');
    expect(pageSource).toContain('setProjectRevision(value => value + 1)');
    expect(coreSource).toContain('LEFT JOIN project_session_binding pb ON pb.session_id = ss.id');
    expect(dbSource).toContain('run V15 project session binding migrations');
    expect(pageSource).not.toContain('writeProvider');
  });
});
