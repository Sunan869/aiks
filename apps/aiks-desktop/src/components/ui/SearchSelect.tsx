import { useEffect, useId, useRef, useState } from "react";
import { Check, ChevronsUpDown, Search } from "lucide-react";

export interface SearchOption { value: string; label: string }
export function SearchSelect({ value, onChange, options, label, placeholder = "选择项目" }: {
  value: string; onChange: (next: string) => void; options: SearchOption[]; label: string; placeholder?: string;
}) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [activeIndex, setActiveIndex] = useState(0);
  const searchInput = useRef<HTMLInputElement>(null);
  const listId = useId();
  const triggerRef = useRef<HTMLButtonElement>(null);
  const root = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const close = (event: PointerEvent) => {
      if (!root.current?.contains(event.target as Node)) setOpen(false);
    };
    document.addEventListener("pointerdown", close);
    return () => document.removeEventListener("pointerdown", close);
  }, [open]);
  useEffect(() => { if (open) searchInput.current?.focus(); }, [open]);
  const matching = options.filter(option => option.label.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase()));
  useEffect(() => {
    if (open && activeIndex >= matching.length) setActiveIndex(Math.max(0, matching.length - 1));
  }, [open, matching.length, activeIndex]);
  const choose = (next: string) => { onChange(next); setOpen(false); setQuery(""); triggerRef.current?.focus(); };
  const closePopup = () => { setOpen(false); setQuery(""); triggerRef.current?.focus(); };
  const handleKeys = (event: React.KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Escape") { event.preventDefault(); event.stopPropagation(); closePopup(); }
    if (event.key === "ArrowDown" && matching.length > 0) { event.preventDefault(); setActiveIndex(index => Math.min(index + 1, matching.length - 1)); }
    if (event.key === "ArrowUp") { event.preventDefault(); setActiveIndex(index => Math.max(0, index - 1)); }
    if (event.key === "Home" && matching.length > 0) { event.preventDefault(); setActiveIndex(0); }
    if (event.key === "End" && matching.length > 0) { event.preventDefault(); setActiveIndex(matching.length - 1); }
    if (event.key === "Enter" && matching[activeIndex]) { event.preventDefault(); choose(matching[activeIndex].value); }
  };
  return <div ref={root} className="relative min-w-0 flex-1">
    <button ref={triggerRef} type="button" role="combobox" aria-label={label} aria-haspopup="listbox" aria-expanded={open} aria-controls={open ? listId : undefined}
      onKeyDown={event => { if (!open && (event.key === "ArrowDown" || event.key === "ArrowUp")) { event.preventDefault(); setOpen(true); setQuery(""); setActiveIndex(0); } }}
      onClick={() => { setOpen(current => !current); setQuery(""); setActiveIndex(0); }}
      className="aiks-field-select flex w-full items-center justify-between gap-2 text-left">
      <span className="truncate">{options.find(option => option.value === value)?.label || placeholder}</span>
      <ChevronsUpDown className="h-4 w-4 shrink-0 text-slate-400"/>
    </button>
    {open && <div data-aiks-popup="true" className="absolute left-0 top-full z-50 mt-2 w-full min-w-0 max-w-[min(90vw,28rem)] rounded-xl border border-slate-200 bg-white p-2 shadow-xl dark:border-slate-700 dark:bg-slate-900"
      onKeyDown={event => { if (event.key === "Escape") { event.stopPropagation(); closePopup(); } }}>
      <label className="flex items-center gap-2 rounded-lg border border-slate-200 px-2 dark:border-slate-700">
        <Search className="h-4 w-4 text-slate-400"/>
        <input ref={searchInput} aria-label={label + "搜索"} value={query} role="searchbox" aria-controls={listId} aria-activedescendant={matching[activeIndex] ? `${listId}-option-${activeIndex}` : undefined} onKeyDown={handleKeys} onChange={event => { setQuery(event.target.value); setActiveIndex(0); }}
          placeholder="输入关键词筛选…" className="min-w-0 flex-1 border-0 bg-transparent py-2 text-sm outline-none"/>
      </label>
      <div id={listId} role="listbox" aria-label={label + "选项"} className="mt-2 max-h-[min(15rem,40vh)] overflow-y-auto">
        {matching.map((option, index) => <button type="button" id={`${listId}-option-${index}`} role="option" tabIndex={-1} aria-selected={value === option.value}
          key={option.value} onMouseEnter={() => setActiveIndex(index)} onClick={() => choose(option.value)}
          className={`flex w-full items-center gap-2 rounded-lg px-2 py-2 text-left text-xs hover:bg-slate-100 dark:hover:bg-slate-800 ${matching[activeIndex]?.value === option.value ? "bg-sky-50 dark:bg-slate-800" : ""}`}>
          <Check className={`h-4 w-4 ${value === option.value ? "text-sky-600" : "invisible"}`}/>
          <span className="truncate">{option.label}</span>
        </button>)}
        {matching.length === 0 && <p role="status" className="p-3 text-xs text-slate-500">没有匹配选项</p>}
      </div>
    </div>}
  </div>;
}
