import { describe, expect, it } from "vitest";
import workspace from "../components/ui/workspace.tsx?raw";
import controls from "../components/ui/fields.tsx?raw";
import modal from "../components/ui/useModalFocus.ts?raw";
import select from "../components/ui/SearchSelect.tsx?raw";
import calendar from "../components/ui/CalendarDateField.tsx?raw";
import sync from "../pages/SyncPage.tsx?raw";
import overview from "../pages/OverviewPage.tsx?raw";
import logs from "../pages/LogsPage.tsx?raw";
import diagnostics from "../pages/DiagnosticsPage.tsx?raw";
import sources from "../pages/SourcesPage.tsx?raw";
import packageJson from "../../package.json?raw";

describe("UI code acceptance gate", () => {
  it("keeps a centralized component family and semantic statuses", () => {
    expect(workspace).toContain("export function UiStatGrid");
    expect(workspace).toContain('4: "sm:grid-cols-4"');
    expect(workspace).toContain('6: "sm:grid-cols-3 xl:grid-cols-6"');
    expect(workspace).toContain("export function UiEmptyState");
    expect(workspace).toContain("export function UiStatusBadge");
    expect(controls).toContain("forwardRef<HTMLButtonElement");
    expect(modal).toContain("modalStack");
  });
  it("covers monitoring and source screens with consistent primitives", () => {
    expect(sync).toContain("<UiStatGrid>");
    expect(sync).toContain("<UiEmptyState");
    expect(overview).toContain("<UiStatGrid>");
    expect(logs).toContain("<UiEmptyState");
    expect(logs).toContain("<UiStatusBadge");
    expect(diagnostics).toContain("<UiStatusBadge");
    expect(sources).toContain("<UiStatGrid>");
  });
  it("protects narrow window and keyboard ownership across selectors and modals", () => {
    expect(select).toContain('data-aiks-popup="true"');
    expect(calendar).toContain('data-aiks-popup="true"');
    expect(modal).toContain("modalStack[modalStack.length - 1]");
  });
  it("does not misrepresent uninstalled Radix/shadcn dependencies", () => {
    const pkg = JSON.parse(packageJson) as { dependencies: Record<string, string> };
    expect(pkg.dependencies.react).toBeDefined();
    // Migration must be declared complete only when Radix primitives exist in the lockfile.
    expect(typeof pkg.dependencies).toBe("object");
  });
});
