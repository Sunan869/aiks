export const BRIDGE_PROTOCOL_VERSION = 1 as const;

export const WORKSPACE_MODES = ["knowledge", "session"] as const;
export type WorkspaceMode = (typeof WORKSPACE_MODES)[number];
export type WorkbenchSurface = WorkspaceMode | "database" | "graph";

export const WORKBENCH_ACTIONS = [
  "showKnowledgeRoot",
  "showSessionRoot",
  "openDocument",
  "openBlock",
  "setWorkspaceMode",
  "showBacklinks",
  "showOutline",
  "showDatabase",
  "showGraph",
  "showSearch",
  "refreshDocument",
] as const;

export type WorkbenchActionName = (typeof WORKBENCH_ACTIONS)[number];

export function isWorkbenchAction(value: string): value is WorkbenchActionName {
  return (WORKBENCH_ACTIONS as readonly string[]).includes(value);
}

export function isWorkspaceMode(value: string): value is WorkspaceMode {
  return (WORKSPACE_MODES as readonly string[]).includes(value);
}

export function boundWorkbenchMode(
  surface: WorkbenchSurface,
  docId?: string | null,
): WorkspaceMode | null {
  if (!docId?.trim()) return null;
  return surface === "knowledge" || surface === "session" ? surface : null;
}

export function shouldKeepWorkbenchMounted(
  page: string,
  _hasSessionDetail: boolean,
): boolean {
  return page === "knowledge";
}
