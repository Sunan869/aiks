import { forwardRef, type ComponentPropsWithoutRef } from "react";
import { CalendarDays, ChevronDown } from "lucide-react";

type SelectProps = ComponentPropsWithoutRef<"select">;
type DateProps = Omit<ComponentPropsWithoutRef<"input">, "type">;

/** Accessible native control with shared design tokens; keeps platform keyboard semantics. */
export const UiSelect = forwardRef<HTMLSelectElement, SelectProps>(function UiSelect(
  { className = "", children, ...props }, ref,
) {
  return (
    <span className="relative inline-flex min-w-0 items-center">
      <select ref={ref} {...props} className={`aiks-field-select appearance-none pr-9 ${className}`}>
        {children}
      </select>
      <ChevronDown aria-hidden="true" className="pointer-events-none absolute right-3 h-4 w-4 text-slate-400" />
    </span>
  );
});

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
