import { LayoutDashboard, BookOpen, Database, Settings, HeartPulse, FileText, GitBranch } from "lucide-react";
import type { Page } from "../App";

interface Props {
  page: Page;
  onNavigate: (p: Page) => void;
  sessionCount: number;
  knowledgeCount: number;
  aiHealthy: boolean;
}

export const MAIN_NAV_PAGE_IDS: Page[] = [
  "overview",
  "sessions",
  "knowledge",
  "processing",
];

const mainNavItems: { id: Page; label: string; icon: React.ElementType }[] = [
  { id: "overview", label: "概览", icon: LayoutDashboard },
  { id: "sessions", label: "工作记录", icon: FileText },
  { id: "knowledge", label: "知识库", icon: BookOpen },
  { id: "processing", label: "处理中心", icon: GitBranch },
];

const bottomNavItems: { id: Page; label: string; icon: React.ElementType }[] = [
  { id: "sources", label: "数据源", icon: Database },
  { id: "settings", label: "设置", icon: Settings },
  { id: "diagnostics", label: "帮助与诊断", icon: HeartPulse },
];

function NavButton({ item, page, onNavigate }: { item: { id: Page; label: string; icon: React.ElementType }; page: Page; onNavigate: (p: Page) => void }) {
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
