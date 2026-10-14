import { describe, expect, it } from "vitest";
import css from "../index.css?raw";
import fields from "../components/ui/fields.tsx?raw";
import settings from "../pages/SettingsPage.tsx?raw";
import memory from "../pages/ProjectMemoryPage.tsx?raw";
describe("UI style cleanup boundaries", () => {
  it("removes obsolete native-date and old switch rules", () => {
    expect(fields).not.toContain('export const DateField');
    expect(css).not.toContain(".aiks-field-date {");
    expect(css).not.toContain(".aiks-button.aiks-switch");
    expect(settings).toContain("<Switch.Root");
  });
  it("targets Radix tabs instead of a deleted nav element", () => {
    expect(memory).toContain("<Tabs.List");
    expect(css).toContain('[role="tablist"][aria-label="项目记忆工作区"]');
    expect(css).not.toContain('.memory-workspace nav[aria-label="项目记忆工作区"]');
  });
  it("keeps responsive shell and shared dropdown/button focus styles", () => {
    expect(css).toContain(".aiks-page-shell { @apply mx-0 w-full min-w-0 max-w-none");
    expect(css).toContain(".aiks-field-select {");
    expect(css).toContain(".aiks-button--outline {");
    expect(css).toContain(".aiks-search-popup input:focus {");
  });
});
