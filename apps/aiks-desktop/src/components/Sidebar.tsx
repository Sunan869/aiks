import type { ElementType } from "react";
import { LayoutDashboard, BookOpen, Database, Settings, HeartPulse, FileText, GitBranch, Folder } from "lucide-react";
import {
  BOTTOM_NAV_ITEMS,
  MAIN_NAV_ITEMS,
  type NavigationItem,
  type Page,
} from "../navigation";

interface Props {
  page: Page;
  onNavigate: (p: Page) => void;
  sessionCount: number;
  knowledgeCount: number;
  aiHealthy: boolean;
}

const icons: Record<Page, ElementType> = {
  overview: LayoutDashboard,
  sessions: FileText,
  knowledge: BookOpen,
  projects: Folder,
  processing: GitBranch,
  sources: Database,
  settings: Settings,
  diagnostics: HeartPulse,
};

type NavItem = NavigationItem & { icon: ElementType };

const mainNavItems: NavItem[] = MAIN_NAV_ITEMS.map(item => ({
  ...item,
  icon: icons[item.id],
}));

const bottomNavItems: NavItem[] = BOTTOM_NAV_ITEMS.map(item => ({
  ...item,
  icon: icons[item.id],
}));

function NavButton({ item, page, onNavigate }: { item: NavItem; page: Page; onNavigate: (p: Page) => void }) {
  const Icon = item.icon;
  const isActive = page === item.id;
  return (
    <button
      onClick={() => onNavigate(item.id)}
      aria-current={isActive ? "page" : undefined}
      className={`mx-2 flex w-[calc(100%-1rem)] items-center gap-3 rounded-lg px-3 py-2.5 text-left text-[13px] transition-all focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-sky-500 ${
        isActive
          ? "bg-slate-900 text-white font-semibold shadow-sm dark:bg-sky-500/15 dark:text-sky-200"
          : "text-slate-600 dark:text-slate-400 hover:bg-slate-100 hover:text-slate-950 dark:hover:bg-slate-800 dark:hover:text-slate-100"
      }`}
    >
      <Icon className="w-4 h-4 flex-shrink-0" />
      {item.label}
    </button>
  );
}

export default function Sidebar({ page, onNavigate, sessionCount, knowledgeCount, aiHealthy }: Props) {
  return (
    <aside className="flex w-56 flex-shrink-0 flex-col border-r border-slate-200/80 bg-white/95 dark:border-slate-800 dark:bg-slate-950" aria-label="主导航">
      <div className="px-5 pb-4 pt-6"><div className="flex h-10 w-10 items-center justify-center rounded-xl bg-slate-900 text-sm font-bold text-white shadow-sm dark:bg-sky-600">AI</div><div className="mt-3 text-sm font-bold tracking-tight text-slate-900 dark:text-white">AI Knowledge Sync</div><div className="text-[11px] text-slate-500">本地知识工作空间</div></div>
      <nav className="flex-1 space-y-1 overflow-y-auto py-2" aria-label="功能导航">
        {mainNavItems.map((item) => (
          <NavButton key={item.id} item={item} page={page} onNavigate={onNavigate} />
        ))}

        <div className="mx-4 my-2 border-t border-gray-100 dark:border-gray-700" />

        {bottomNavItems.map((item) => (
          <NavButton key={item.id} item={item} page={page} onNavigate={onNavigate} />
        ))}
      </nav>

      <div className="px-4 py-3 border-t border-gray-100 dark:border-gray-700 space-y-1 text-xs text-gray-400">
        <div className="flex justify-between">
          <span>工作记录</span>
          <span className="font-medium text-gray-600 dark:text-gray-300">{sessionCount}</span>
        </div>
        <div className="flex justify-between">
          <span>知识条目</span>
          <span className="font-medium text-gray-600 dark:text-gray-300">{knowledgeCount}</span>
        </div>
        <div className="flex items-center gap-1 mt-1">
          <span className={`w-1.5 h-1.5 rounded-full ${aiHealthy ? "bg-green-500" : "bg-yellow-400"}`} />
          <span className="text-xs">{aiHealthy ? "AI 正常" : "AI 不可用"}</span>
        </div>
      </div>
    </aside>
  );
}
