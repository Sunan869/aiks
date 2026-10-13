import { describe, expect, it } from "vitest";
import combo from "../components/ui/SearchSelect.tsx?raw";
import calendar from "../components/ui/CalendarDateField.tsx?raw";
import workbench from "../pages/KnowledgeWorkbenchPage.tsx?raw";
import about from "../pages/AboutPage.tsx?raw";
describe("desktop shared interaction accessibility", () => {
  it("supports searchable keyboard selection", () => {
    expect(combo).toContain('event.key === "ArrowDown"');
    expect(combo).toContain('event.key === "ArrowUp"');
    expect(combo).toContain('<Command.List role="listbox"');
    expect(combo).toContain('<Command.Input');
    expect(combo).toContain("onSelect={() => choose(option.value)}");
    expect(combo).toContain("triggerRef.current?.focus()");
    expect(combo).toContain('role="combobox"');
  });
  it("returns calendar focus on Escape and closes while disabled", () => {
    expect(calendar).toContain("trigger.current?.focus()");
    expect(calendar).toContain('<DayPicker');
    expect(calendar).toContain("if (disabled) setOpen(false)");
  });
  it("retains knowledge filtering and unifies the selection control", () => {
    expect(workbench).toContain("UiSelect");
    expect(workbench).toContain("知识库");
  });
  it("uses the application page shell for credits", () => {
    expect(about).toContain("aiks-page-shell");
  });
});
