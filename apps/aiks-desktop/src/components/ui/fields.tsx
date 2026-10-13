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

type ButtonProps = ComponentPropsWithoutRef<"button"> & { variant?: "primary" | "secondary" | "outline" | "ghost" | "danger"; size?: "sm" | "md" };
export const UiButton = forwardRef<HTMLButtonElement, ButtonProps>(function UiButton(
  { variant = "secondary", size = "md", className = "", type = "button", ...props }, ref,
) {
  return <button ref={ref} type={type} {...props} className={`aiks-button aiks-button--${variant} aiks-button--${size} ${className}`} />;
});
