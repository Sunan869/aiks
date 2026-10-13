import { describe, expect, it } from "vitest";
import fields from "../components/ui/fields.tsx?raw";
import editor from "../components/KnowledgeEditor.tsx?raw";
import settings from "../pages/SettingsPage.tsx?raw";
describe("composable Tailwind primitives", () => {
  it("uses CVA variants and mergeable utilities, preserving existing button APIs", () => {
    expect(fields).toContain('import { cva } from "class-variance-authority"');
    expect(fields).toContain('import { twMerge } from "tailwind-merge"');
    expect(fields).toContain('className={cn(buttonStyles({ variant, size }), className)}');
    for (const variant of ["primary","secondary","outline","ghost","danger"]) expect(fields).toContain("aiks-button--"+variant);
  });
  it("retains controlled Radix semantics", () => {
    expect(settings).toContain("onCheckedChange={onChange}");
    expect(editor).toContain('onValueChange={value => setPreview(value === "preview")}');
  });
});
