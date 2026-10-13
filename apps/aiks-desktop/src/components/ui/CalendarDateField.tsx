import { useEffect, useId, useRef, useState } from "react";
import { CalendarDays, ChevronLeft, ChevronRight } from "lucide-react";

interface Props {
  value: string;
  onChange: (next: string) => void;
  label: string;
  disabled?: boolean;
}
const pad = (value: number) => String(value).padStart(2, "0");
const iso = (date: Date) => `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
const parse = (value: string) => {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value);
  if (!match) return null;
  const date = new Date(Number(match[1]), Number(match[2]) - 1, Number(match[3]));
  return iso(date) === value ? date : null;
};

/** Controlled, accessible calendar popover; dates remain ISO strings. */
export function CalendarDateField({ value, onChange, label, disabled }: Props) {
  const [open, setOpen] = useState(false);
  const [month, setMonth] = useState(() => {
    const current = parse(value) ?? new Date();
    return new Date(current.getFullYear(), current.getMonth(), 1);
  });
  const root = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const yearMonthLabel = useId();
  const dayButtons = useRef<HTMLDivElement>(null);
  const id = useId();
  useEffect(() => {
    if (!open) return;
    const close = (event: PointerEvent) => {
      if (!root.current?.contains(event.target as Node)) setOpen(false);
    };
    document.addEventListener("pointerdown", close);
    return () => document.removeEventListener("pointerdown", close);
  }, [open]);
  useEffect(() => { if (disabled) setOpen(false); }, [disabled]);
  useEffect(() => {
    if (!open) return;
    const selected = dayButtons.current?.querySelector<HTMLButtonElement>('button[aria-pressed="true"]');
    (selected ?? dayButtons.current?.querySelector<HTMLButtonElement>("button[data-day]"))?.focus();
  }, [open]);
  const first = new Date(month.getFullYear(), month.getMonth(), 1).getDay();
  const days = new Date(month.getFullYear(), month.getMonth() + 1, 0).getDate();
  const cells = Array.from({ length: Math.ceil((first + days) / 7) * 7 }, (_, index) => index - first + 1);
  const move = (offset: number) => setMonth(current => new Date(current.getFullYear(), current.getMonth() + offset, 1));
  const monthTitle = `${month.getFullYear()} 年 ${month.getMonth() + 1} 月`;
  const selectDate = (day: Date) => {
    onChange(iso(day));
    setOpen(false);
    trigger.current?.focus();
  };
  const moveFocus = (event: React.KeyboardEvent<HTMLDivElement>) => {
    const delta: Record<string, number> = { ArrowLeft: -1, ArrowRight: 1, ArrowUp: -7, ArrowDown: 7 };
    const step = delta[event.key];
    if (event.key === "Home" || event.key === "End") {
      const buttons = Array.from(dayButtons.current?.querySelectorAll<HTMLButtonElement>("button[data-day]") ?? []);
      event.preventDefault();
      buttons[event.key === "Home" ? 0 : buttons.length - 1]?.focus();
      return;
    }
    if (step === undefined) return;
    const buttons = Array.from(dayButtons.current?.querySelectorAll<HTMLButtonElement>("button[data-day]") ?? []);
    const selectedIndex = buttons.indexOf(document.activeElement as HTMLButtonElement);
    if (selectedIndex < 0) return;
    event.preventDefault();
    buttons[Math.max(0, Math.min(buttons.length - 1, selectedIndex + step))]?.focus();
  };
  return (
    <div ref={root} className="relative inline-block min-w-0 w-full sm:w-auto sm:min-w-[180px]">
      <button type="button" aria-label={label} aria-haspopup="dialog" aria-expanded={open} aria-controls={open ? id : undefined}
        ref={trigger} disabled={disabled} onClick={() => { setMonth(parse(value) ? new Date(Number(value.slice(0, 4)), Number(value.slice(5, 7)) - 1, 1) : new Date(new Date().getFullYear(), new Date().getMonth(), 1)); setOpen(current => !current); }}
        className="aiks-field-date flex w-full items-center gap-2 px-3 text-left">
        <CalendarDays className="h-4 w-4 shrink-0 text-slate-500" />
        <span className="flex-1">{value || "选择日期"}</span>
      </button>
      {open && (
        <div id={id} data-aiks-popup="true" role="dialog" aria-modal="false" aria-label={label + "日历"} aria-labelledby={yearMonthLabel}
          onKeyDown={event => { if (event.key === "Escape") { event.preventDefault(); event.stopPropagation(); setOpen(false); trigger.current?.focus(); } }}
          className="absolute left-0 top-full z-50 mt-2 w-[min(18rem,90vw)] max-w-full rounded-xl border border-slate-200 bg-white p-3 shadow-xl dark:border-slate-700 dark:bg-slate-900">
          <div className="mb-3 flex items-center justify-between">
            <button type="button" aria-label="上个月" onClick={() => move(-1)} className="rounded-lg p-2 hover:bg-slate-100 dark:hover:bg-slate-800"><ChevronLeft className="h-4 w-4"/></button>
            <strong id={yearMonthLabel} aria-live="polite" className="text-sm">{monthTitle}</strong>
            <button type="button" aria-label="下个月" onClick={() => move(1)} className="rounded-lg p-2 hover:bg-slate-100 dark:hover:bg-slate-800"><ChevronRight className="h-4 w-4"/></button>
          </div>
          <div ref={dayButtons} onKeyDown={moveFocus} className="grid grid-cols-7 gap-1 text-center text-xs">
            {["日", "一", "二", "三", "四", "五", "六"].map(day => <span key={day} className="py-1 text-slate-500">{day}</span>)}
            {cells.map((day, index) => day <= 0 || day > days
              ? <span key={index} aria-hidden="true" />
              : <button key={index} type="button"
                  aria-label={`${month.getFullYear()}年${month.getMonth() + 1}月${day}日`}
                  data-day={day} aria-pressed={value === iso(new Date(month.getFullYear(), month.getMonth(), day))}
                  onClick={() => selectDate(new Date(month.getFullYear(), month.getMonth(), day))}
                  className={`rounded-lg py-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-sky-500 hover:bg-sky-100 dark:hover:bg-slate-700 ${value === iso(new Date(month.getFullYear(), month.getMonth(), day)) ? "bg-sky-600 font-bold text-white hover:bg-sky-700" : ""}`}>
                  {day}
                </button>)}
          </div>
          <div className="mt-3 flex justify-end border-t border-slate-100 pt-2 dark:border-slate-800">
            <button type="button" className="rounded-lg px-3 py-1.5 text-xs text-sky-700 dark:text-sky-300"
              onClick={() => selectDate(new Date())}>今天</button>
          </div>
        </div>
      )}
    </div>
  );
}
