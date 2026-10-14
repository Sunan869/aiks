import { describe, expect, it } from "vitest";
import legacy from "../pages/KnowledgePage.tsx?raw";
import native from "../pages/KnowledgeBasePageV3.tsx?raw";

describe("knowledge pages desktop shell acceptance", () => {
  it("keeps both knowledge entry points keyboard accessible", () => {
    expect(legacy).toContain("aiks-page-shell");
    expect(legacy).toContain("打开知识库查看会话");
    expect(legacy).toContain('type="button" onClick={() => void openKnowledge()}');
    expect(native).toContain("查看知识：");
    expect(native).toContain("syncKnowledgeToSiyuan");
  });
});
