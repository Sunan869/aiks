import { describe, expect, it } from "vitest";
import picker from "../components/ui/SearchSelect.tsx?raw";
import calendar from "../components/ui/CalendarDateField.tsx?raw";
import processing from "../pages/ProcessingPage.tsx?raw";

describe("narrow-window UI safeguards", () => {
  it("keeps popovers bounded and keyboard navigation intact", () => {
    expect(picker).toContain("max-w-[min(90vw,28rem)]");
    expect(picker).toContain("<Popover.Content");
    expect(picker).toContain("collisionPadding={8}");
    expect(picker).toContain("<Command.List");
    expect(picker).toContain("onSelect={() => choose(option.value)}");
    expect(picker).toContain('onEscapeKeyDown={event => { event.stopPropagation(); }}');
    expect(calendar).toContain("w-[min(18rem,90vw)]");
    expect(calendar).toContain("<Popover.Content");
    expect(calendar).toContain("collisionPadding={8}");
    expect(calendar).toContain("focus-visible:ring-2");
    expect(calendar).toContain('aria-live="polite"');
    expect(calendar).toContain('month_grid: "w-full border-collapse"');
    expect(calendar).toContain("event.stopPropagation(); setOpen(false)");
  });
  it("keeps pipeline retry and table visibility controls", () => {
    expect(processing).toContain("retryFailedAiTask");
    expect(processing).toContain("aiks-data-table");
  });
});
