import { useEffect, useId, useRef, useState } from "react";
import * as Popover from "@radix-ui/react-popover";
import { CalendarDays } from "lucide-react";
import { DayPicker } from "react-day-picker";

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
  const trigger = useRef<HTMLButtonElement>(null);
  const yearMonthLabel = useId();
  const id = useId();
  useEffect(() => { if (disabled) setOpen(false); }, [disabled]);
  useEffect(() => {
    if (!open) return;
    const activeDate = parse(value);
    if (activeDate) setMonth(new Date(activeDate.getFullYear(), activeDate.getMonth(), 1));
  }, [value, open]);
  const activeDate = parse(value);
  const today = new Date();
  const selectDate = (day: Date) => {
    onChange(iso(day));
    setOpen(false);
    trigger.current?.focus();
  };
  const monthTitle = `${month.getFullYear()} 年 ${month.getMonth() + 1} 月`;
  return (
    <Popover.Root open={open} onOpenChange={setOpen}>
      <div className="relative inline-block min-w-0 w-full sm:w-auto sm:min-w-[180px]">
      <Popover.Anchor asChild><button type="button" aria-label={label} aria-haspopup="dialog" aria-expanded={open} aria-controls={open ? id : undefined}
        ref={trigger} disabled={disabled} onClick={() => { setMonth(parse(value) ? new Date(Number(value.slice(0, 4)), Number(value.slice(5, 7)) - 1, 1) : new Date(new Date().getFullYear(), new Date().getMonth(), 1)); setOpen(current => !current); }}
        className="aiks-field-date flex w-full items-center gap-2 px-3 text-left">
        <CalendarDays className="h-4 w-4 shrink-0 text-slate-500" />
        <span className="flex-1">{value || "选择日期"}</span>
      </button></Popover.Anchor>
      <Popover.Portal>
        <Popover.Content id={id} data-aiks-popup="true" aria-labelledby={yearMonthLabel} side="bottom" align="start" sideOffset={8} collisionPadding={8}
          onOpenAutoFocus={event => { event.preventDefault(); }}
          onCloseAutoFocus={event => { event.preventDefault(); trigger.current?.focus(); }}
          onKeyDown={event => { if (event.key === "Escape") { event.stopPropagation(); setOpen(false); trigger.current?.focus(); } }}
          className="z-[110] w-[min(18rem,90vw)] max-w-full rounded-xl border border-slate-200 bg-white p-3 shadow-xl dark:border-slate-700 dark:bg-slate-900">
          <strong id={yearMonthLabel} aria-live="polite" className="mb-2 block text-center text-sm">{monthTitle}</strong>
          <DayPicker
            mode="single"
            month={month}
            onMonthChange={setMonth}
            selected={activeDate ?? undefined}
            onSelect={day => { if (day) selectDate(day); }}
            today={today}
            showOutsideDays={false}
            classNames={{
              root: "text-sm text-slate-800 dark:text-slate-100",
              months: "w-full",
              month: "w-full",
              month_caption: "flex items-center justify-between pb-2",
              caption_label: "sr-only",
              nav: "flex items-center gap-1",
              button_previous: "rounded-lg border border-slate-200 px-2 py-1.5 hover:bg-slate-100 focus-visible:ring-2 focus-visible:ring-sky-500 dark:border-slate-700 dark:hover:bg-slate-800",
              button_next: "rounded-lg border border-slate-200 px-2 py-1.5 hover:bg-slate-100 focus-visible:ring-2 focus-visible:ring-sky-500 dark:border-slate-700 dark:hover:bg-slate-800",
              month_grid: "w-full border-collapse",
              weekdays: "grid grid-cols-7",
              weekday: "py-1 text-center text-xs font-medium text-slate-500",
              week: "grid grid-cols-7",
              day: "p-0.5",
              day_button: "h-9 w-9 rounded-lg text-xs transition-colors hover:bg-sky-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-sky-500 dark:hover:bg-slate-700",
              selected: "rounded-lg bg-sky-600 text-white [&_button]:hover:bg-sky-700",
              today: "font-semibold text-sky-600 dark:text-sky-400",
              outside: "invisible",
              disabled: "opacity-40",
            }}
          />
          <div className="mt-3 flex justify-end border-t border-slate-100 pt-2 dark:border-slate-800">
            <button type="button" className="rounded-lg px-3 py-1.5 text-xs text-sky-700 dark:text-sky-300"
              onClick={() => selectDate(new Date())}>今天</button>
          </div>
        </Popover.Content>
      </Popover.Portal>
      </div>
    </Popover.Root>
  );
}
