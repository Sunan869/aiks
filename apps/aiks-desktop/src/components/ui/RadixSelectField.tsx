import * as Select from "@radix-ui/react-select";
import { Check, ChevronDown, ChevronUp } from "lucide-react";

export interface RadixChoice { value: string; label: string; disabled?: boolean }

interface Props {
  value: string;
  onValueChange: (value: string) => void;
  options: RadixChoice[];
  label: string;
  disabled?: boolean;
  placeholder?: string;
  className?: string;
}

/** A Radix-backed select for fixed choices; use SearchSelect for very long searchable lists. */
export function RadixSelectField({ value, onValueChange, options, label, disabled, placeholder = "请选择", className = "" }: Props) {
  // Radix reserves the empty item value for clearing. Use a sentinel for native 'all' choices.
  const empty = "__aiks_empty__";
  const toRadix = (option: string) => option === "" ? empty : option;
  const fromRadix = (option: string) => option === empty ? "" : option;
  return (
    <Select.Root value={toRadix(value)} onValueChange={next => onValueChange(fromRadix(next))} disabled={disabled}>
      <Select.Trigger aria-label={label} className={`aiks-field-select inline-flex min-w-0 items-center justify-between gap-2 text-left focus-visible:ring-2 focus-visible:ring-sky-500/30 ${className}`}>
        <Select.Value placeholder={placeholder} />
        <Select.Icon><ChevronDown className="h-4 w-4 text-slate-400" /></Select.Icon>
      </Select.Trigger>
      <Select.Portal>
        <Select.Content position="popper" sideOffset={5} collisionPadding={8} className="z-[110] max-h-[min(20rem,60vh)] min-w-[var(--radix-select-trigger-width)] max-w-[min(90vw,36rem)] overflow-hidden rounded-xl border border-slate-200 bg-white p-1 text-slate-900 shadow-xl dark:border-slate-700 dark:bg-slate-900 dark:text-slate-100">
          <Select.ScrollUpButton className="flex justify-center p-1"><ChevronUp className="h-4 w-4" /></Select.ScrollUpButton>
          <Select.Viewport className="p-1">
            {options.map(option => (
              <Select.Item key={option.value} value={toRadix(option.value)} disabled={option.disabled} className="relative flex min-h-9 cursor-pointer select-none items-center rounded-md py-2 pl-8 pr-3 text-sm outline-none focus:bg-sky-50 data-[disabled]:opacity-40 dark:focus:bg-slate-800">
                <Select.ItemIndicator className="absolute left-2"><Check className="h-4 w-4" /></Select.ItemIndicator>
                <Select.ItemText>{option.label}</Select.ItemText>
              </Select.Item>
            ))}
          </Select.Viewport>
          <Select.ScrollDownButton className="flex justify-center p-1"><ChevronDown className="h-4 w-4" /></Select.ScrollDownButton>
        </Select.Content>
      </Select.Portal>
    </Select.Root>
  );
}
