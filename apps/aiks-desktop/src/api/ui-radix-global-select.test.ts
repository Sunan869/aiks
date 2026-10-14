import { describe, expect, it } from "vitest";
import fields from "../components/ui/fields.tsx?raw";
import radix from "../components/ui/RadixSelectField.tsx?raw";
import settings from "../pages/SettingsPage.tsx?raw";
import sessions from "../pages/SessionsPage.tsx?raw";
import memory from "../pages/ProjectMemoryPage.tsx?raw";
import knowledge from "../pages/KnowledgeWorkbenchPage.tsx?raw";
describe("Radix Select global native dropdown migration", () => {
  it("routes legacy UiSelect through a real Radix Select", () => {
    expect(fields).toContain("function extractOptions");
    expect(fields).toContain("child.type !== \"option\"");
    expect(fields).toContain("return <RadixSelectField");
    expect(fields).toContain("onChange?.({ target: { value: next }");
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
