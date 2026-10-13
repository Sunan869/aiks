import { useEffect, useId, useRef, useState } from "react";
import * as Popover from "@radix-ui/react-popover";
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

  useEffect(() => { if (open) searchInput.current?.focus(); }, [open]);
  const matching = options.filter(option => option.label.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase()));
  const selectedIndex = matching.findIndex(option => option.value === value);
  useEffect(() => {
    if (open && activeIndex >= matching.length) setActiveIndex(Math.max(0, matching.length - 1));
  }, [open, matching.length, activeIndex]);
  const choose = (next: string) => { onChange(next); setOpen(false); setQuery(""); triggerRef.current?.focus(); };
  const closePopup = () => { setOpen(false); setQuery(""); triggerRef.current?.focus(); };
  const openPopup = () => { setOpen(true); setQuery(""); setActiveIndex(Math.max(0, selectedIndex)); };
  const handleKeys = (event: React.KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Escape") { event.preventDefault(); event.stopPropagation(); closePopup(); return; }
    if (event.key === "ArrowDown" && matching.length > 0) { event.preventDefault(); setActiveIndex(index => Math.min(index + 1, matching.length - 1)); }
    if (event.key === "ArrowUp") { event.preventDefault(); setActiveIndex(index => Math.max(0, index - 1)); }
    if (event.key === "Home" && matching.length > 0) { event.preventDefault(); setActiveIndex(0); }
    if (event.key === "End" && matching.length > 0) { event.preventDefault(); setActiveIndex(matching.length - 1); }
    if (event.key === "Enter" && matching[activeIndex]) { event.preventDefault(); choose(matching[activeIndex].value); }
  };
  return <Popover.Root open={open} onOpenChange={next => { setOpen(next); if (!next) setQuery(""); }}>
    <div className="relative min-w-0 flex-1">
    <Popover.Anchor asChild><button ref={triggerRef} type="button" role="combobox" aria-label={label} aria-haspopup="listbox" aria-expanded={open} aria-controls={open ? listId : undefined} aria-autocomplete="list"
      onKeyDown={event => { if (!open && (event.key === "ArrowDown" || event.key === "ArrowUp")) { event.preventDefault(); openPopup(); } }}
      onClick={() => { if (open) closePopup(); else openPopup(); }}
      className="aiks-field-select flex w-full items-center justify-between gap-2 text-left">
      <span className="truncate">{options.find(option => option.value === value)?.label || placeholder}</span>
      <ChevronsUpDown className="h-4 w-4 shrink-0 text-slate-400"/>
    </button></Popover.Anchor>
    <Popover.Portal><Popover.Content data-aiks-popup="true" side="bottom" align="start" sideOffset={8} collisionPadding={8} onOpenAutoFocus={event => { event.preventDefault(); searchInput.current?.focus(); }} onCloseAutoFocus={event => { event.preventDefault(); triggerRef.current?.focus(); }} className="z-[110] w-[var(--radix-popover-trigger-width)] min-w-0 max-w-[min(90vw,28rem)] rounded-xl border border-slate-200 bg-white p-2 shadow-xl outline-none dark:border-slate-700 dark:bg-slate-900"
      onKeyDown={event => { if (event.key === "Escape") { event.stopPropagation(); closePopup(); } }}>
      <label className="aiks-search-popup flex items-center gap-2 rounded-lg border border-slate-200 bg-slate-50 px-2 focus-within:border-sky-400 focus-within:ring-2 focus-within:ring-sky-500/15 dark:border-slate-700 dark:bg-slate-800">
        <Search className="h-4 w-4 text-slate-400"/>
        <input ref={searchInput} aria-label={label + "搜索"} value={query} role="searchbox" aria-controls={listId} aria-activedescendant={matching[activeIndex] ? `${listId}-option-${activeIndex}` : undefined} onKeyDown={handleKeys} onChange={event => { setQuery(event.target.value); setActiveIndex(0); }}
          placeholder="输入关键词筛选…" className="min-w-0 flex-1 border-0 bg-transparent py-2 text-sm shadow-none outline-none focus:border-transparent focus:outline-none focus:ring-0"/>
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
    </Popover.Content></Popover.Portal>
    </div>
  </Popover.Root>;
}
