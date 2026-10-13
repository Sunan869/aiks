import { describe, expect, it } from "vitest";
import hook from "../components/ui/useModalFocus.ts?raw";
import editor from "../components/KnowledgeEditor.tsx?raw";
import search from "../components/UnifiedSearchDialog.tsx?raw";
import ask from "../components/AskAiksPanel.tsx?raw";
import select from "../components/ui/SearchSelect.tsx?raw";
import calendar from "../components/ui/CalendarDateField.tsx?raw";

describe("modal keyboard boundary", () => {
  it("traps navigation and restores previously focused trigger", () => {
    expect(hook).toContain('event.key !== "Tab"');
    expect(hook).toContain("event.shiftKey");
    expect(hook).toContain("previous.focus()");
    expect(hook).toContain("enabledRef.current");
    expect(hook).toContain("modalStack[modalStack.length - 1] !== token");
    expect(hook).toContain("source.closest(");
  });
  it("applies the same focus boundary to knowledge, search and assistant", () => {
    expect(editor).toContain("useModalFocus(open, onClose, !saving)");
    expect(search).toContain("useModalFocus(open, onClose)");
    expect(ask).toContain("useModalFocus<HTMLElement>(open, onClose)");
    for (const content of [editor, search, ask]) expect(content).toContain("ref={dialogRef}");
    expect(editor).toContain("createKnowledge(");
    expect(search).toContain("searchAllProgressively");
    expect(ask).toContain("askAiksProgressively");
  });
  it("lets nested selectors handle Escape before the parent dialog", () => {
    expect(select).toContain('data-aiks-popup="true"');
    expect(calendar).toContain('data-aiks-popup="true"');
    expect(select).toContain("closePopup()");
    expect(calendar).toContain("trigger.current?.focus()");
    expect(calendar).toContain("const activeDate = parse(value)");
    expect(select).toContain("setQuery(\"\")");
    expect(hook).toContain("wasTopmost");
    expect(hook).toContain('source.closest(\'[data-aiks-popup="true"]\')');
  });
});
