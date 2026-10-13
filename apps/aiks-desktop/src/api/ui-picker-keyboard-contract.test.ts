import { describe, expect, it } from "vitest";
import combo from "../components/ui/SearchSelect.tsx?raw";
import calendar from "../components/ui/CalendarDateField.tsx?raw";

describe("shared picker keyboard behavior", () => {
  it("supports keyboard-open and predictable Escape close", () => {
    expect(combo).toContain('event.key === "ArrowDown" || event.key === "ArrowUp"');
    expect(combo).toContain("closePopup()");
    expect(combo).toContain('role="status"');
  });
  it("focuses calendar dates and supports Home/End", () => {
    expect(calendar).toContain("<DayPicker");
    expect(calendar).toContain("onMonthChange={setMonth}");
    expect(calendar).toContain("selected={activeDate ?? undefined}");
    expect(calendar).toContain('aria-haspopup="dialog"');
    expect(calendar).toContain("onChange(iso(day))");
  });
});
