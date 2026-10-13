import { useEffect, useRef, useState } from "react";
import { Check, ChevronsUpDown, Search } from "lucide-react";

export interface SearchOption { value: string; label: string }
export function SearchSelect({ value, onChange, options, label, placeholder = "选择项目" }: {
  value: string; onChange: (next: string) => void; options: SearchOption[]; label: string; placeholder?: string;
}) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const root = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const close = (event: PointerEvent) => {
      if (!root.current?.contains(event.target as Node)) setOpen(false);
    };
    document.addEventListener("pointerdown", close);
    return () => document.removeEventListener("pointerdown", close);
  }, [open]);
  const matching = options.filter(option => option.label.toLocaleLowerCase().includes(query.toLocaleLowerCase()));
  return <div ref={root} className="relative min-w-0 flex-1">
    <button type="button" role="combobox" aria-label={label} aria-expanded={open}
      onClick={() => { setOpen(current => !current); setQuery(""); }}
      className="aiks-field-select flex w-full items-center justify-between gap-2 text-left">
      <span className="truncate">{options.find(option => option.value === value)?.label || placeholder}</span>
      <ChevronsUpDown className="h-4 w-4 shrink-0 text-slate-400"/>
    </button>
    {open && <div className="absolute left-0 top-full z-50 mt-2 w-full min-w-[240px] rounded-xl border border-slate-200 bg-white p-2 shadow-xl dark:border-slate-700 dark:bg-slate-900"
      onKeyDown={event => { if (event.key === "Escape") setOpen(false); }}>
      <label className="flex items-center gap-2 rounded-lg border border-slate-200 px-2 dark:border-slate-700">
        <Search className="h-4 w-4 text-slate-400"/>
        <input aria-label={label + "搜索"} autoFocus value={query} onChange={event => setQuery(event.target.value)}
          placeholder="输入关键词筛选…" className="min-w-0 flex-1 border-0 bg-transparent py-2 text-sm outline-none"/>
      </label>
      <div role="listbox" aria-label={label + "选项"} className="mt-2 max-h-60 overflow-y-auto">
        {matching.map(option => <button type="button" role="option" aria-selected={value === option.value}
          key={option.value} onClick={() => { onChange(option.value); setOpen(false); }}
          className="flex w-full items-center gap-2 rounded-lg px-2 py-2 text-left text-xs hover:bg-slate-100 dark:hover:bg-slate-800">
          <Check className={`h-4 w-4 ${value === option.value ? "text-sky-600" : "invisible"}`}/>
          <span className="truncate">{option.label}</span>
        </button>)}
        {matching.length === 0 && <p className="p-3 text-xs text-slate-500">没有匹配项目</p>}
      </div>
    </div>}
  </div>;
}
