import { describe, expect, it } from "vitest";
import buttons from "../components/ui/fields.tsx?raw";
import sources from "../pages/SourcesPage.tsx?raw";
import config from "../../tailwind.config.js?raw";

describe("Tailwind shared control survival", () => {
  it("keeps every button variant literal and safelisted", () => {
    for (const name of ["primary", "secondary", "outline", "ghost", "danger"]) {
      expect(buttons).toContain(`aiks-button--${name}`);
      expect(config).toContain(`"aiks-button--${name}"`);
    }
    expect(buttons).toContain("buttonVariants[variant]");
  });
  it("stretches source cards per grid row", () => {
    expect(sources).toContain("aiks-source-grid grid grid-cols-1 items-stretch");
    expect(sources).toContain("flex h-full min-w-0 flex-col");
  });
});
