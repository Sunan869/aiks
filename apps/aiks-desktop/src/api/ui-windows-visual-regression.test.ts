import { describe, expect, it } from "vitest";
import css from "../index.css?raw";
import sidebar from "../components/Sidebar.tsx?raw";
import sources from "../pages/SourcesPage.tsx?raw";
import select from "../components/ui/SearchSelect.tsx?raw";
describe("visual corrections from Windows acceptance", () => {
  it("uses full width pages without a redundant navigation logo tile", () => {
    expect(css).toContain(".aiks-page-shell { @apply mx-0 w-full max-w-none");
    expect(css).toContain(".memory-workspace { @apply w-full min-w-0 max-w-none");
    expect(sidebar).not.toContain('>AI</div>');
  });
  it("aligns variable-length data source cards", () => {
    expect(sources).toContain("flex h-full min-w-0 flex-col");
    expect(sources).toContain("mt-auto flex flex-wrap");
  });
  it("provides visible outline buttons and a contained search focus ring", () => {
    expect(css).toContain(".aiks-button--outline");
    expect(select).toContain("aiks-search-popup");
    expect(select).toContain("focus:ring-0");
  });
});
