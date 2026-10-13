import type { ReactNode } from "react";

export type StatusTone = "neutral" | "success" | "warning" | "danger";
const toneClasses: Record<StatusTone, string> = {
  neutral: "bg-slate-100 text-slate-700 dark:bg-slate-800 dark:text-slate-200",
  success: "bg-emerald-100 text-emerald-800 dark:bg-emerald-950 dark:text-emerald-300",
  warning: "bg-amber-100 text-amber-800 dark:bg-amber-950 dark:text-amber-300",
  danger: "bg-red-100 text-red-700 dark:bg-red-950 dark:text-red-300",
};

export function UiStatusBadge({ children, tone = "neutral" }: { children: ReactNode; tone?: StatusTone }) {
  return <span className={`inline-flex max-w-full items-center rounded-full px-2.5 py-1 text-xs font-medium ${toneClasses[tone]}`}>{children}</span>;
}

export function UiEmptyState({ title, description, actions }: { title: string; description?: ReactNode; actions?: ReactNode }) {
  return <div className="flex min-h-32 min-w-0 flex-col items-center justify-center gap-2 rounded-xl border border-dashed border-slate-300 p-6 text-center dark:border-slate-700" role="status">
    <p className="text-sm font-medium text-slate-700 dark:text-slate-200">{title}</p>
    {description && <p className="max-w-lg text-xs text-slate-500 dark:text-slate-400">{description}</p>}
    {actions && <div className="mt-2 flex flex-wrap justify-center gap-2">{actions}</div>}
  </div>;
}

export function UiStatGrid({ children, columns = 4 }: { children: ReactNode; columns?: 3 | 4 | 6 }) {
  const responsive = { 3: "sm:grid-cols-3", 4: "sm:grid-cols-4", 6: "sm:grid-cols-3 xl:grid-cols-6" }[columns];
  return <div className={`grid min-w-0 grid-cols-2 gap-3 ${responsive}`}>{children}</div>;
}
