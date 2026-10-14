import { describe, expect, it } from "vitest";
import modal from "../components/ui/ModalBackdrop.tsx?raw";
import combo from "../components/ui/SearchSelect.tsx?raw";
import select from "../components/ui/RadixSelectField.tsx?raw";
describe("shared Radix interaction guardrails", () => {
  it("keeps in-flight editor dialogs open for outside interactions", () => {
    expect(modal).toContain("onPointerDownOutside");
    expect(modal).toContain("onInteractOutside");
    expect(modal).toContain("if (!dismissOnBackdrop) event.preventDefault()");
    expect(modal).toContain("onEscapeKeyDown");
  });
  it("preserves searchable choice identity while searching by label", () => {
    expect(combo).toContain("value={commandValueForOption(option.value)}");
    expect(combo).toContain("uniqueChoiceValues(options)");
    expect(combo).toContain("keywords={[option.label, option.value]}");
    expect(combo).toContain("onSelect={() => choose(option.value)}");
    expect(combo).toContain("closePopup();");
  });
  it("retains the all-choice sentinel mapping", () => {
    expect(select).toContain("emptyOptionSentinel(choices)");
    expect(select).toContain("uniqueChoiceValues(options)");
    expect(select).toContain('option === "" ? blank : option');
    expect(select).toContain('option === blank ? "" : option');
  });
});