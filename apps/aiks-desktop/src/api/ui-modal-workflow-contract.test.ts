import { describe, expect, it } from "vitest";
import editor from "../components/KnowledgeEditor.tsx?raw";
import search from "../components/UnifiedSearchDialog.tsx?raw";

describe("responsive modal workflow contracts", () => {
  it("keeps knowledge save operations and protects in-flight writes", () => {
    expect(editor).toContain("updateKnowledge(initial.id");
    expect(editor).toContain("createKnowledge(");
    expect(editor).toContain('aria-labelledby="knowledge-editor-title"');
    expect(editor).toContain('disabled={saving}');
    expect(editor).toContain('event.key === "Escape" && !saving');
    expect(editor).toContain('md:grid-cols-[260px_minmax(0,1fr)]');
  });
  it("keeps progressive global search in a labelled dialog", () => {
    expect(search).toContain("searchAllProgressively");
    expect(search).toContain('aria-label="全局搜索"');
    expect(search).toContain('aria-label="搜索知识和 AI 对话记录"');
    expect(search).toContain("controller.abort()");
  });
});
