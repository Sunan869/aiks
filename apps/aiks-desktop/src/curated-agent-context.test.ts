import { describe, expect, it } from "vitest";
import { MockAiksApi } from "./api/mock";
import projectPage from "./pages/ProjectMemoryPage.tsx?raw";
import tauriApi from "./api/tauri.ts?raw";
import rustCommand from "../src-tauri/src/project_memory_commands.rs?raw";
import core from "../../../crates/aiks-core/src/knowledge/project_outputs.rs?raw";

describe("S3.3 user-selected project Agent context packs", () => {
  it("exports only approved, explicitly chosen knowledge IDs in review order", async () => {
    const api = new MockAiksApi();
    const projects = await api.getProjectMemories();
    const matches = await Promise.all(projects.map(async project => ({
      project,
      detail: await api.getProjectMemory(project.id),
    })));
    const selected = matches.find(({ detail }) =>
      detail.entries.filter(entry =>
        entry.feedback_status !== "incorrect" && entry.feedback_status !== "outdated"
      ).length >= 2
    );
    expect(selected).toBeDefined();
    const chosen = selected!.detail.entries.filter(item =>
      item.feedback_status !== "incorrect" && item.feedback_status !== "outdated"
    ).slice(0, 2);
    const output = await api.createCuratedAgentContextPack(
      selected!.project.id, 1000, undefined, [chosen[1].knowledge_id, chosen[0].knowledge_id],
    );
    expect(output).toContain(chosen[1].knowledge_id);
    expect(output).toContain(chosen[0].knowledge_id);
    expect(output.indexOf(chosen[1].knowledge_id))
      .toBeLessThan(output.indexOf(chosen[0].knowledge_id));
    const unselected = selected!.detail.entries.find(item =>
      !chosen.some(k => k.knowledge_id === item.knowledge_id)
    );
    if (unselected) expect(output).not.toContain("来源：知识 " + unselected.knowledge_id);
    const original = await api.getKnowledgeDetail(chosen[0].knowledge_id);
    expect(original.id).toBe(chosen[0].knowledge_id);
  });

  it("refuses cross-project, duplicate, stale and source-mismatched IDs", async () => {
    const api = new MockAiksApi();
    const projects = await api.getProjectMemories();
    const chosenProject = projects[0];
    const snapshot = await api.getProjectMemory(chosenProject.id);
    const candidate = snapshot.entries[0];
    expect(candidate).toBeDefined();
    await expect(api.createCuratedAgentContextPack(
      chosenProject.id, 1000, undefined, [],
    )).rejects.toThrow();
    await expect(api.createCuratedAgentContextPack(
      chosenProject.id, 1000, undefined, [candidate.knowledge_id, candidate.knowledge_id],
    )).rejects.toThrow();
    await expect(api.createCuratedAgentContextPack(
      chosenProject.id, 1000, undefined, ["missing-source"],
    )).rejects.toThrow();
    await expect(api.createCuratedAgentContextPack(
      chosenProject.id, 1000, "made-up-provider", [candidate.knowledge_id],
    )).rejects.toThrow();
    const different = projects.find(item => item.id !== chosenProject.id);
    expect(different).toBeDefined();
    await expect(api.createCuratedAgentContextPack(
      different!.id, 1000, undefined, [candidate.knowledge_id],
    )).rejects.toThrow();
    await expect(api.createCuratedAgentContextPack(
      chosenProject.id, 1000, undefined, Array.from({ length: 31 }, (_, index) => String(index)),
    )).rejects.toThrow();
  });

  it("rejects current human-invalidated knowledge instead of silently omitting the selection", async () => {
    const api = new MockAiksApi();
    const projects = await api.getProjectMemories();
    const chosen = await Promise.all(projects.map(async project => ({
      project,
      entries: (await api.getProjectMemory(project.id)).entries,
    })));
    const one = chosen.find(item => item.entries.length > 0);
    expect(one).toBeDefined();
    const knowledgeId = one!.entries[0].knowledge_id;
    await api.addKnowledgeFeedback(knowledgeId, "outdated", "Manual review revoked reuse");
    await expect(api.createCuratedAgentContextPack(
      one!.project.id, 1024, undefined, [knowledgeId],
    )).rejects.toThrow();
    await expect(api.createCuratedAgentContextPack(
      one!.project.id, 1024, undefined, [knowledgeId, "missing"],
    )).rejects.toThrow();
  });

  it("requires a second authoritative backend scope check, never edits instructions", () => {
    expect(projectPage).toContain("createCuratedAgentContextPack(");
    expect(projectPage).toContain("人工精选 Agent 知识包（最多 30 条）");
    expect(projectPage).toContain("selectedAgentKnowledgeIds.length >= 30");
    expect(projectPage).toContain('item.feedback_status !== "incorrect"');
    expect(projectPage).toContain("renderAgentSuggestion(output, agentFormat)");
    expect(tauriApi).toContain('invoke("create_curated_agent_context_pack"');
    expect(rustCommand).toContain("render_curated_agent_context_scoped(");
    expect(rustCommand).toContain("get_scoped(&project_id, 200, source.as_deref())");
    expect(core).toContain("Selected knowledge not in current project scope");
    expect(core).toContain("Knowledge flagged incorrect or outdated cannot be exported");
    expect(core).toContain("curated_agent_context_honors_explicit_order_and_source_boundaries");
    expect(core).toContain("Selected knowledge exceeds the configured Agent context budget");
    expect(projectPage).not.toContain("writeFile(AGENTS");
  });
});
