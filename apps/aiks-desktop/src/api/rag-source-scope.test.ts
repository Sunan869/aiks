import { describe, expect, it } from "vitest";
import { MockAiksApi } from "./mock";
import type { RagAskRequest } from "./rag";
import panelSource from "../components/AskAiksPanel.tsx?raw";
import rustRag from "../../../../crates/aiks-core/src/rag.rs?raw";

describe("S2.4 source-scoped evidence-only Ask AIKS", () => {
  it("uses source, project and both corpora in unified local retrieval", async () => {
    const api = new MockAiksApi();
    const all = await api.searchAll("AIKS Task", { corpora: ["session"], limit: 100 });
    expect(all.hits.length).toBeGreaterThan(0);
    const codex = await api.searchAll("AIKS Task", {
      corpora: ["session"], source: "codex", limit: 100,
    });
    expect(codex.hits.length).toBeGreaterThan(0);
    expect(codex.hits.every(hit => hit.snippet.startsWith("codex ·"))).toBe(true);
    const scoped = await api.searchAll("AIKS Task", {
      corpora: ["session"], source: "codex", project: "Desktop", limit: 100,
    });
    expect(scoped.hits.length).toBeGreaterThan(0);
    expect(scoped.hits.every(hit =>
      hit.snippet.startsWith("codex · Desktop")
    )).toBe(true);
    expect((await api.searchAll("AIKS Task", {
      corpora: ["session"], source: "unknown-provider",
    })).hits).toEqual([]);
  });

  it("sends exact source with project and returns citations only from matching evidence", async () => {
    const api = new MockAiksApi();
    const scoped: RagAskRequest = { question: "AIKS Task", source: "codex" };
    const answer = await api.askAiks(scoped);
    expect(answer.citations.length).toBeGreaterThan(0);
    expect(answer.citations.every(citation =>
      citation.corpus === "session" && citation.snippet.startsWith("codex ·")
    )).toBe(true);
    const noMatch = await api.askAiks({ question: "AIKS Task", source: "bad-source" });
    expect(noMatch.citations).toEqual([]);
    expect(noMatch.answer).toContain("依据");
  });

  it("clears prior conversational history when a user changes either evidence filter", () => {
    expect(panelSource).toContain('aria-label="问 AIKS 项目范围"');
    expect(panelSource).toContain('aria-label="问 AIKS 数据源范围"');
    expect(panelSource).toContain('source: selectedSource || undefined');
    expect(panelSource).toContain('project: selectedProject || undefined');
    expect(panelSource).toMatch(/setSelectedProject\(value\);\s*setMessages\(\[\]\);/);
    expect(panelSource).toMatch(/setSelectedSource\(value\);\s*setMessages\(\[\]\);/);
    expect(rustRag).toContain("let scope = retrieval_scope(&request)?");
    expect(rustRag).toContain("source: normalize(\"source\", &request.source)?");
    expect(rustRag).toContain("audit_answer_citations(&answer, &self.citations)");
  });
});
