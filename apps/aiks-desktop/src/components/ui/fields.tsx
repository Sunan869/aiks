import { Children, Fragment, forwardRef, isValidElement, useState, type ChangeEvent, type ComponentPropsWithoutRef, type ReactNode } from "react";
import { RadixSelectField, type RadixChoice } from "./RadixSelectField";
import { CalendarDays, ChevronDown } from "lucide-react";

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
  return <input ref={ref} {...props} className={`aiks-field-input ${className}`} />;
});

export const UiTextarea = forwardRef<HTMLTextAreaElement, ComponentPropsWithoutRef<"textarea">>(function UiTextarea(
  { className = "", ...props }, ref,
) {
  return <textarea ref={ref} {...props} className={`aiks-field-input resize-y ${className}`} />;
});

type ButtonVariant = "primary" | "secondary" | "outline" | "ghost" | "danger";
type ButtonSize = "sm" | "md";
type ButtonProps = ComponentPropsWithoutRef<"button"> & { variant?: ButtonVariant; size?: ButtonSize };
// Literal class references are required for Tailwind 3 content scanning.
// Template-based class construction caused the shared action styles to be purged.
const buttonVariants: Record<ButtonVariant, string> = {
  primary: "aiks-button--primary",
  secondary: "aiks-button--secondary",
  outline: "aiks-button--outline",
  ghost: "aiks-button--ghost",
  danger: "aiks-button--danger",
};
const buttonSizes: Record<ButtonSize, string> = {
  sm: "aiks-button--sm",
  md: "aiks-button--md",
};
export const UiButton = forwardRef<HTMLButtonElement, ButtonProps>(function UiButton(
  { variant = "secondary", size = "md", className = "", type = "button", ...props }, ref,
) {
  return <button ref={ref} type={type} {...props} className={`aiks-button ${buttonVariants[variant]} ${buttonSizes[size]} ${className}`} />;
});
