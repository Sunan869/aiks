import type { ReactNode } from "react";

type Tone = "neutral" | "success" | "warning" | "danger";

const noticeStyles: Record<Tone, string> = {
  neutral: "border-slate-200 bg-slate-50 text-slate-700 dark:border-slate-700 dark:bg-slate-900 dark:text-slate-200",
  success: "border-emerald-200 bg-emerald-50 text-emerald-800 dark:border-emerald-900 dark:bg-emerald-950/30 dark:text-emerald-200",
  warning: "border-amber-200 bg-amber-50 text-amber-800 dark:border-amber-900 dark:bg-amber-950/30 dark:text-amber-200",
  danger: "border-red-200 bg-red-50 text-red-700 dark:border-red-900 dark:bg-red-950/30 dark:text-red-200",
};

export function UiNotice({ children, tone = "neutral", className = "" }: { children: ReactNode; tone?: Tone; className?: string }) {
  return <div role={tone === "danger" ? "alert" : "status"} className={`min-w-0 break-words rounded-lg border px-3 py-2 text-xs ${noticeStyles[tone]} ${className}`}>{children}</div>;
}

export function UiSection({ title, description, children, className = "" }: {
  title: string; description?: ReactNode; children: ReactNode; className?: string;
}) {
  return <section className={`aiks-surface min-w-0 ${className}`} aria-label={title}>
    <h2 className="text-sm font-semibold tracking-tight text-slate-900 dark:text-slate-100">{title}</h2>
    {description && <p className="mt-1 text-xs text-slate-500 dark:text-slate-400">{description}</p>}
    <div className="mt-3 min-w-0">{children}</div>
  </section>;
}

export function UiMetric({ label, value, detail, tone = "neutral" }: {
  label: string; value: ReactNode; detail?: ReactNode; tone?: Tone;
}) {
  const valueTone = { neutral: "text-slate-900 dark:text-slate-100", success: "text-emerald-600 dark:text-emerald-300", warning: "text-amber-600 dark:text-amber-300", danger: "text-red-600 dark:text-red-300" }[tone];
  return <div className="min-w-0 rounded-xl border border-slate-200 bg-white/90 p-3 dark:border-slate-700 dark:bg-slate-900/80">
    <div className="text-xs text-slate-500 dark:text-slate-400">{label}</div>
    <div className={`mt-1 text-xl font-semibold tabular-nums ${valueTone}`}>{value}</div>
    {detail && <div className="mt-1 text-xs text-slate-500 dark:text-slate-400">{detail}</div>}
  </div>;
}
