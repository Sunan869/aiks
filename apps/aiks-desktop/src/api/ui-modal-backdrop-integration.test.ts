import { describe, expect, it } from "vitest";
import backdrop from "../components/ui/ModalBackdrop.tsx?raw";
import editor from "../components/KnowledgeEditor.tsx?raw";
import search from "../components/UnifiedSearchDialog.tsx?raw";
import assistant from "../components/AskAiksPanel.tsx?raw";
describe("shared modal backdrop integration", () => {
  it("uses target-only outside dismissal rather than bubbling child clicks", () => {
    expect(backdrop).toContain("<Dialog.Root open");
    expect(backdrop).toContain("<Dialog.Overlay");
    expect(backdrop).toContain("<Dialog.Content asChild");
    expect(backdrop).toContain("if (!dismissOnBackdrop) event.preventDefault()");
    expect(editor).toContain('dismissOnBackdrop={!saving}');
  });
  it("preserves existing modal accessibility hooks in all three workflows", () => {
    for (const component of [editor, search, assistant]) {
      expect(component).toContain("<ModalBackdrop");
      expect(component).toContain("</ModalBackdrop>");
      expect(component).toContain('role="dialog" aria-modal="true"');
      expect(component).toContain("ref={dialogRef}");
    }
    expect(editor).toContain("createKnowledge(");
    expect(search).toContain("searchAllProgressively");
    expect(assistant).toContain("askAiksProgressively");
  });
});
