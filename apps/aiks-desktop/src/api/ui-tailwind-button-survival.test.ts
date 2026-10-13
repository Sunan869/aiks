import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import buttons from "../components/ui/fields.tsx?raw";
import sources from "../pages/SourcesPage.tsx?raw";
const config = readFileSync(new URL("../../tailwind.config.js", import.meta.url), "utf8");
const styles = readFileSync(new URL("../index.css", import.meta.url), "utf8");

describe("Tailwind shared control survival", () => {
  it("keeps every button variant literal and safelisted", () => {
    for (const name of ["primary", "secondary", "outline", "ghost", "danger"]) {
      expect(buttons).toContain(`aiks-button--${name}`);
      expect(config).toContain(`"aiks-button--${name}"`);
      expect(styles).toContain(`.aiks-button--${name}`);
    }
    expect(buttons).toContain("buttonVariants[variant]");
  });
  it("stretches source cards per grid row", () => {
    expect(sources).toContain("aiks-source-grid grid grid-cols-1 items-stretch");
    expect(sources).toContain("flex h-full min-w-0 flex-col");
  });
});
