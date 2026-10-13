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
