import type { WorkbenchSurface, WorkspaceMode } from "./api/workbench";

export const WORKBENCH_MAIN_MODES = ["document", "database", "graph"] as const;
export type WorkbenchMainMode = (typeof WORKBENCH_MAIN_MODES)[number];

export function resolveWorkbenchSurface(
  mainMode: WorkbenchMainMode,
  workspaceMode: WorkspaceMode,
): WorkbenchSurface {
  return mainMode === "document" ? workspaceMode : mainMode;
}
