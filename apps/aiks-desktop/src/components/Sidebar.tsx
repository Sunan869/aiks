import type { ElementType } from "react";
import { LayoutDashboard, BookOpen, Database, Settings, HeartPulse, FileText, GitBranch } from "lucide-react";
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
      className={`w-full flex items-center gap-2.5 px-4 py-2 text-sm transition-colors ${
        isActive
          ? "bg-blue-50 dark:bg-blue-900/30 text-blue-700 dark:text-blue-300 font-medium border-r-2 border-blue-600"
          : "text-gray-600 dark:text-gray-400 hover:bg-gray-50 dark:hover:bg-gray-700"
      }`}
    >
      <Icon className="w-4 h-4 flex-shrink-0" />
      {item.label}
    </button>
  );
}

export default function Sidebar({ page, onNavigate, sessionCount, knowledgeCount, aiHealthy }: Props) {
  return (
    <div className="w-48 flex-shrink-0 flex flex-col bg-white dark:bg-gray-800 border-r border-gray-200 dark:border-gray-700">
      <nav className="flex-1 py-2">
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
    </div>
  );
}
