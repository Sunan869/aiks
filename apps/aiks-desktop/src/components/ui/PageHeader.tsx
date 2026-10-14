import type { ReactNode } from "react";

interface PageHeaderProps {
  title: string;
  description?: ReactNode;
  actions?: ReactNode;
  className?: string;
}

/** Common header for the local-first desktop workspaces. */
export function PageHeader({ title, description, actions, className = "" }: PageHeaderProps) {
  return (
    <header className={`mb-5 flex min-w-0 flex-wrap items-start justify-between gap-3 ${className}`}>
      <div className="min-w-0">
        <h1 className="text-2xl font-bold tracking-tight text-slate-900 dark:text-slate-50">{title}</h1>
        {description && <div className="mt-1 text-sm text-slate-500 dark:text-slate-400">{description}</div>}
      </div>
      {actions && <div className="flex min-w-0 flex-wrap items-center gap-2">{actions}</div>}
    </header>
  );
}
