import { describe, expect, it } from "vitest";
import detail from "../pages/KnowledgeDetailPage.tsx?raw";
import memory from "../pages/ProjectMemoryPage.tsx?raw";
describe("Radix detail and memory selection contracts", () => {
  it("migrates fixed choices and preserves organization reset safeguards", () => {
    expect(detail.match(/<RadixSelectField/g)?.length).toBe(3);
    expect(detail).toContain('setOrganizeOperation(value as AiAssistOperation)');
    expect(detail).toContain('setOrganizeConfirmed(false)');
    expect(detail).toContain('role="group" aria-label="多来源知识选择"');
  });
  it("preserves project binding and report review semantics", () => {
    expect(memory.match(/<RadixSelectField/g)?.length).toBeGreaterThanOrEqual(6);
    expect(memory).toContain('onValueChange={setBindingSession}');
    expect(memory).toContain('onValueChange={setBindingTarget}');
    expect(memory).toContain('setSelectedAgentKnowledgeIds([])');
    expect(memory).toContain('openSavedReport(reportDraftId!,Number(value))');
  });
});