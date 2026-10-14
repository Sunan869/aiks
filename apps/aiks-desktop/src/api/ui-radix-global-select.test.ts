import { describe, expect, it } from "vitest";
import fields from "../components/ui/fields.tsx?raw";
import radix from "../components/ui/RadixSelectField.tsx?raw";
import settings from "../pages/SettingsPage.tsx?raw";
import sessions from "../pages/SessionsPage.tsx?raw";
import memory from "../pages/ProjectMemoryPage.tsx?raw";
import knowledge from "../pages/KnowledgeWorkbenchPage.tsx?raw";
describe("Radix Select global native dropdown migration", () => {
  it("uses real Radix Select without legacy synthetic DOM change events", () => {
    expect(fields).not.toContain("function extractOptions");
    expect(fields).not.toContain("onChange?.({ target: { value: next }");
    expect(fields).not.toContain("export function UiSelect");
    expect(radix).toContain("<Select.Root");
    expect(radix).toContain("<Select.Portal>");
    expect(radix).toContain("onValueChange(fromRadix(next))");
  });
  it("keeps original business filter and settings bindings", () => {
    expect(settings).toContain("<RadixSelectField");
    expect(sessions).toContain("setSource(value)");
    expect(memory).toContain("onValueChange={setBindingSession}");
    expect(knowledge).toContain("setCategory(value)");
  });
});
