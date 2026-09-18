export type Page =
  | "overview"
  | "sessions"
  | "knowledge"
  | "processing"
  | "sources"
  | "settings"
  | "diagnostics";

export type WorkbenchRouteMode = "knowledge" | "session";

export interface NavState {
  page: Page;
  sessionDetailId?: number;
  knowledgeDetailId?: string;
  pipelineDetailRunId?: string;
  workbenchMode?: WorkbenchRouteMode;
  workbenchDocId?: string;
}

export interface NavigationItem {
  id: Page;
  label: string;
}

export const MAIN_NAV_ITEMS: readonly NavigationItem[] = [
  { id: "overview", label: "概览" },
  { id: "sessions", label: "工作记录" },
  { id: "knowledge", label: "知识库" },
  { id: "processing", label: "处理中心" },
];

export const BOTTOM_NAV_ITEMS: readonly NavigationItem[] = [
  { id: "sources", label: "数据源" },
  { id: "settings", label: "设置" },
  { id: "diagnostics", label: "帮助与诊断" },
];

export function rawConversationNavState(docId?: string | null): NavState {
  const normalizedDocId = docId?.trim();
  return {
    page: "knowledge",
    workbenchMode: "session",
    ...(normalizedDocId ? { workbenchDocId: normalizedDocId } : {}),
  };
}
