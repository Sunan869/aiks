import { describe, expect, it } from "vitest";
import { commandValueForOption, emptyOptionSentinel, uniqueChoiceValues } from "../components/ui/selectionIdentity";
import { formatLocalIsoDate, localMonthStart, parseLocalIsoDate } from "../components/ui/calendarDates";
describe("picker domain identity", () => {
  it("keeps case-distinct, Unicode and ambiguous options independently selectable", () => {
    const values = ["", "A", "a", "x y", "x  y", "中", "😀", "😀A", "__aiks_empty__"];
    expect(new Set(values.map(commandValueForOption)).size).toBe(values.length);
    expect(commandValueForOption("A")).not.toBe(commandValueForOption("a"));
    expect(commandValueForOption("😀A")).toBe(commandValueForOption("😀A"));
  });
  it("deduplicates domain keys but preserves distinct keys with identical labels", () => {
    const options = [{value: "a", label: "First"}, {value: "b", label: "Same"},
      {value: "a", label: "Duplicate"}, {value: "c", label: "Same"}];
    expect(uniqueChoiceValues(options).map(item => item.label)).toEqual(["First", "Same", "Same"]);
    expect(options).toHaveLength(4);
  });
  it("chooses a non-colliding sentinel despite multiple reserved-looking options", () => {
    const values = ["", "__aiks_empty__", "__aiks_empty___", "__aiks_empty____"];
    const choices = values.map(value => ({ value }));
    const sentinel = emptyOptionSentinel(choices);
    expect(values).not.toContain(sentinel);
    expect(sentinel).not.toBe("");
    expect(emptyOptionSentinel([...choices].reverse())).toBe(sentinel);
  });
});
describe("calendar date-only boundaries", () => {
  it("rejects invalid dates and preserves leap days", () => {
    for (const value of ["2025-02-29", "2024-02-30", "2024-13-01", "2024-2-01", "invalid"])
      expect(parseLocalIsoDate(value)).toBeNull();
    expect(formatLocalIsoDate(parseLocalIsoDate("2024-02-29")!)).toBe("2024-02-29");
  });
  it("preserves two-digit years and month rollover without UTC conversion", () => {
    const old = parseLocalIsoDate("0099-12-31");
    expect(old?.getFullYear()).toBe(99);
    expect(formatLocalIsoDate(old!)).toBe("0099-12-31");
    expect(formatLocalIsoDate(localMonthStart(parseLocalIsoDate("2026-12-31")!))).toBe("2026-12-01");
  });
});
