import { Children, Fragment, forwardRef, isValidElement, useState, type ChangeEvent, type ComponentPropsWithoutRef, type ReactNode } from "react";
import { RadixSelectField, type RadixChoice } from "./RadixSelectField";
import { CalendarDays } from "lucide-react";
import { clsx } from "clsx";
import { twMerge } from "tailwind-merge";
import { cva } from "class-variance-authority";

/** Composable Tailwind tokens; merger prevents ad-hoc page classes from fighting defaults. */
export const cn = (...values: Parameters<typeof clsx>) => twMerge(clsx(...values));
const buttonStyles = cva(
  "aiks-button inline-flex items-center justify-center gap-2 whitespace-nowrap rounded-lg border font-medium shadow-sm transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-sky-500 focus-visible:ring-offset-2 disabled:pointer-events-none disabled:opacity-50 dark:focus-visible:ring-offset-slate-950",
  { variants: {
      variant: {
        primary: "aiks-button--primary border-sky-600 bg-sky-600 font-semibold text-white hover:bg-sky-700",
        secondary: "aiks-button--secondary border-slate-200 bg-slate-100 text-slate-800 hover:bg-slate-200 dark:border-slate-700 dark:bg-slate-800 dark:text-slate-100 dark:hover:bg-slate-700",
        outline: "aiks-button--outline border-slate-300 bg-white text-slate-800 hover:bg-slate-50 dark:border-slate-600 dark:bg-slate-900 dark:text-slate-100 dark:hover:bg-slate-800",
        ghost: "aiks-button--ghost border-transparent bg-transparent text-slate-600 shadow-none hover:bg-slate-100 dark:text-slate-300 dark:hover:bg-slate-800",
        danger: "aiks-button--danger border-red-600 bg-red-600 text-white hover:bg-red-700",
      },
      size: { sm: "aiks-button--sm min-h-[34px] px-3 py-1.5 text-xs", md: "aiks-button--md min-h-[38px] px-4 py-2 text-sm" },
    }, defaultVariants: { variant: "secondary", size: "md" } }
);
const inputStyles = "aiks-field-input min-h-[38px] min-w-0 rounded-lg border border-slate-300 bg-white px-3 py-2 text-sm text-slate-900 shadow-sm outline-none transition-colors placeholder:text-slate-400 focus-visible:border-sky-500 focus-visible:ring-2 focus-visible:ring-sky-500/30 disabled:cursor-not-allowed disabled:opacity-50 dark:border-slate-700 dark:bg-slate-900 dark:text-slate-100";
type SelectProps = ComponentPropsWithoutRef<"select">;
type DateProps = Omit<ComponentPropsWithoutRef<"input">, "type">;

/** Read existing JSX <option> children so all existing screens can migrate in one batch. */
function extractOptions(children: ReactNode): RadixChoice[] {
  const result: RadixChoice[] = [];
  const visit = (nodes: ReactNode) => {
    Children.forEach(nodes, child => {
      if (!isValidElement(child)) return;
      if (child.type === Fragment) {
        visit((child.props as { children?: ReactNode }).children);
        return;
      }
      if (child.type !== "option") return;
      const props = child.props as { value?: string | number; children?: ReactNode; disabled?: boolean };
      const optionValue = String(props.value ?? props.children ?? "");
      const content = Children.toArray(props.children).map(item => typeof item === "string" || typeof item === "number" ? String(item) : "").join("");
      result.push({ value: optionValue, label: content || optionValue, disabled: props.disabled });
    });
  };
  visit(children);
  return result;
}

/**
 * Radix-backed compatibility adapter for all existing select call sites.
 * Keep the legacy ChangeEvent-like onChange contract until business screens
 * migrate individually to onValueChange.
 */
export function UiSelect({
  className = "", children, value, defaultValue, onChange, disabled, "aria-label": ariaLabel,
  id, name, required,
}: SelectProps) {
  const options = extractOptions(children);
  const [uncontrolled, setUncontrolled] = useState(String(defaultValue ?? options[0]?.value ?? ""));
  const currentValue = value !== undefined && value !== null ? String(value) : uncontrolled;
  const onValueChange = (next: string) => {
    if (value === undefined || value === null) setUncontrolled(next);
    onChange?.({ target: { value: next }, currentTarget: { value: next } } as ChangeEvent<HTMLSelectElement>);
  };
  return <RadixSelectField
    label={String(ariaLabel ?? name ?? id ?? "选择选项")}
    value={currentValue}
    onValueChange={onValueChange}
    options={options}
    disabled={disabled}
    name={name}
    required={required}
    className={className}
  />;
}

/** Consistent calendar icon and focus treatment while retaining OS-native date input. */
export const DateField = forwardRef<HTMLInputElement, DateProps>(function DateField(
  { className = "", ...props }, ref,
) {
  return (
    <span className="relative inline-flex min-w-0 items-center">
      <CalendarDays aria-hidden="true" className="pointer-events-none absolute left-3 h-4 w-4 text-slate-400" />
      <input ref={ref} type="date" {...props} className={`aiks-field-date pl-9 ${className}`} />
    </span>
  );
});

/** Shared focus and disabled-state inputs for native keyboard and IME behavior. */
export const UiInput = forwardRef<HTMLInputElement, ComponentPropsWithoutRef<"input">>(function UiInput(
  { className = "", ...props }, ref,
) {
  return <input ref={ref} {...props} className={cn(inputStyles, className)} />;
});

export const UiTextarea = forwardRef<HTMLTextAreaElement, ComponentPropsWithoutRef<"textarea">>(function UiTextarea(
  { className = "", ...props }, ref,
) {
  return <textarea ref={ref} {...props} className={cn(inputStyles, "resize-y", className)} />;
});

type ButtonVariant = "primary" | "secondary" | "outline" | "ghost" | "danger";
type ButtonSize = "sm" | "md";
type ButtonProps = ComponentPropsWithoutRef<"button"> & { variant?: ButtonVariant; size?: ButtonSize };
export const UiButton = forwardRef<HTMLButtonElement, ButtonProps>(function UiButton(
  { variant = "secondary", size = "md", className = "", type = "button", ...props }, ref,
) {
  return <button ref={ref} type={type} {...props} className={cn(buttonStyles({ variant, size }), className)} />;
});
