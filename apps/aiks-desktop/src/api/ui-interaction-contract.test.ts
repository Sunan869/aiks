import { describe, expect, it } from "vitest";
import memorySource from "../pages/ProjectMemoryPage.tsx?raw";
import sessionSource from "../pages/SessionsPage.tsx?raw";
import calendarSource from "../components/ui/CalendarDateField.tsx?raw";
import comboSource from "../components/ui/SearchSelect.tsx?raw";

describe("shared calendar and searchable selection contract", () => {
  it("keeps project selection searchable and preserves project identity", () => {
    expect(memorySource).toContain('<SearchSelect label="项目" value={selected} onChange={setSelected}');
    expect(comboSource).toContain('const choose = (next: string) => { onChange(next);');
    expect(comboSource).toContain('onSelect={() => choose(option.value)}');
    expect(comboSource).toContain('<Command.List role="listbox"');
    expect(comboSource).toContain('role="combobox"');
  });
  it("keeps ISO dates and invalidates existing report previews", () => {
    expect(memorySource).toContain('<CalendarDateField label="回顾开始日期"');
    expect(memorySource).toContain('<CalendarDateField label="回顾结束日期"');
    expect(memorySource).toContain('invalidatePreview(); setFrom(value)');
    expect(memorySource).toContain('invalidatePreview(); setThrough(value)');
    expect(calendarSource).toContain('<DayPicker');
    expect(calendarSource).toContain('onMonthChange={setMonth}');
    expect(calendarSource).toContain('onSelect={day => { if (day) selectDate(day); }}');
    expect(calendarSource).toContain('onChange(iso(day))');
    expect(calendarSource).toContain('onClick={() => selectDate(new Date())}');
  });
  it("keeps session source filters connected", () => {
    expect(sessionSource).toContain('onChange={e => { setSource(e.target.value); setPage(0); }}');
    expect(sessionSource).toContain('<UiSelect aria-label="工作记录来源"');
  });
});
