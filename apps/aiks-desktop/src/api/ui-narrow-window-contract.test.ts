import { describe, expect, it } from "vitest";
import picker from "../components/ui/SearchSelect.tsx?raw";
import calendar from "../components/ui/CalendarDateField.tsx?raw";
import processing from "../pages/ProcessingPage.tsx?raw";

describe("narrow-window UI safeguards", () => {
  it("keeps popovers bounded and keyboard navigation intact", () => {
    expect(picker).toContain("max-w-[min(90vw,28rem)]");
    expect(picker).toContain("activeIndex >= matching.length");
    expect(picker).toContain('event.key === "Enter"');
    expect(calendar).toContain("w-[min(18rem,90vw)]");
    expect(calendar).toContain("focus-visible:ring-2");
  });
  it("keeps pipeline retry and table visibility controls", () => {
    expect(processing).toContain("retryFailedAiTask");
    expect(processing).toContain("aiks-data-table");
  });
});
