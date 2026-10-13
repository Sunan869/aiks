import { useRef, useState } from "react";
import * as Popover from "@radix-ui/react-popover";
import { Command } from "cmdk";
import { Check, ChevronsUpDown, Search } from "lucide-react";

export interface SearchOption { value: string; label: string }

/** Radix Popover + cmdk combobox. Command handles filtering, arrow navigation, and Enter. */
export function SearchSelect({ value, onChange, options, label, placeholder = "选择项目" }: {
  value: string; onChange: (next: string) => void; options: SearchOption[]; label: string; placeholder?: string;
}) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const triggerRef = useRef<HTMLButtonElement>(null);
  const searchInput = useRef<HTMLInputElement>(null);
  const choose = (next: string) => {
    onChange(next);
    setOpen(false);
    setQuery("");
    triggerRef.current?.focus();
  };
  const closePopup = () => {
    setOpen(false);
    setQuery("");
    triggerRef.current?.focus();
  };
  const openPopup = () => { setQuery(""); setOpen(true); };

  return (
    <Popover.Root open={open} onOpenChange={next => { setOpen(next); if (!next) setQuery(""); }}>
      <div className="relative min-w-0 flex-1">
        <Popover.Anchor asChild>
          <button ref={triggerRef} type="button" role="combobox" aria-label={label}
            aria-expanded={open} aria-haspopup="listbox"
            onKeyDown={event => {
              if (!open && (event.key === "ArrowDown" || event.key === "ArrowUp")) {
                event.preventDefault();
                openPopup();
              }
            }}
            onClick={() => { if (open) closePopup(); else openPopup(); }}
            className="aiks-field-select flex w-full items-center justify-between gap-2 text-left">
            <span className="truncate">{options.find(option => option.value === value)?.label || placeholder}</span>
            <ChevronsUpDown className="h-4 w-4 shrink-0 text-slate-400" />
          </button>
        </Popover.Anchor>
        <Popover.Portal>
          <Popover.Content data-aiks-popup="true" side="bottom" align="start" sideOffset={8} collisionPadding={8}
            onOpenAutoFocus={event => { event.preventDefault(); searchInput.current?.focus(); }}
            onCloseAutoFocus={event => { event.preventDefault(); triggerRef.current?.focus(); }}
            onEscapeKeyDown={event => { event.stopPropagation(); }}
            className="z-[110] w-[var(--radix-popover-trigger-width)] min-w-0 max-w-[min(90vw,28rem)] rounded-xl border border-slate-200 bg-white p-2 shadow-xl outline-none dark:border-slate-700 dark:bg-slate-900">
            <Command label={label + "选项"} loop className="min-w-0">
              <div className="aiks-search-popup flex items-center gap-2 rounded-lg border border-slate-200 bg-slate-50 px-2 focus-within:border-sky-400 focus-within:ring-2 focus-within:ring-sky-500/15 dark:border-slate-700 dark:bg-slate-800">
                <Search className="h-4 w-4 text-slate-400" />
                <Command.Input ref={searchInput} aria-label={label + "搜索"} value={query}
                  onValueChange={setQuery} placeholder="输入关键词筛选…"
                  className="min-w-0 flex-1 border-0 bg-transparent py-2 text-sm shadow-none outline-none focus:border-transparent focus:outline-none focus:ring-0" />
              </div>
              <Command.List role="listbox" className="mt-2 max-h-[min(15rem,40vh)] overflow-y-auto">
                <Command.Empty role="status" className="p-3 text-xs text-slate-500">没有匹配选项</Command.Empty>
                {options.map(option => (
                  <Command.Item key={option.value} value={option.value}
                    keywords={[option.label]} onSelect={() => choose(option.value)}
                    className="flex min-h-9 cursor-pointer items-center gap-2 rounded-lg px-2 py-2 text-left text-xs outline-none data-[selected=true]:bg-sky-50 dark:data-[selected=true]:bg-slate-800">
                    <Check className={`h-4 w-4 ${value === option.value ? "text-sky-600" : "invisible"}`} />
                    <span className="truncate">{option.label}</span>
                  </Command.Item>
                ))}
              </Command.List>
            </Command>
          </Popover.Content>
        </Popover.Portal>
      </div>
    </Popover.Root>
  );
}
