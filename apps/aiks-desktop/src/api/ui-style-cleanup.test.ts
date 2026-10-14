import { describe, expect, it } from "vitest";
import fields from "../components/ui/fields.tsx?raw";
import settings from "../pages/SettingsPage.tsx?raw";
import memory from "../pages/ProjectMemoryPage.tsx?raw";
import picker from "../components/ui/RadixSelectField.tsx?raw";
import search from "../components/ui/SearchSelect.tsx?raw";

describe("UI style cleanup boundaries", () => {
  it("removes obsolete native picker code while keeping Radix switches", () => {
    expect(fields).not.toContain("export const DateField");
    expect(fields).not.toContain("export function UiSelect");
    expect(settings).toContain("<Switch.Root");
    expect(settings).toContain("<Switch.Thumb");
  });
  it("keeps project memory navigation as genuine Radix tabs", () => {
    expect(memory).toContain("<Tabs.List");
    expect(memory).toContain('aria-label="项目记忆工作区"');
    expect(memory).toContain("<Tabs.Trigger");
    expect(memory).not.toContain('<nav aria-label="项目记忆工作区"');
  });
  it("keeps responsive shared control and popup styling at component boundaries", () => {
    expect(fields).toContain("min-h-[38px]");
    expect(fields).toContain("aiks-button--outline");
    expect(picker).toContain("aiks-field-select");
    expect(picker).toContain("focus-visible:ring-2");
    expect(search).toContain("aiks-search-popup");
    expect(search).toContain("focus:ring-0");
  });
});
