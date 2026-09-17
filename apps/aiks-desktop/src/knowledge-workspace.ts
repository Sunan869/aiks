import type { WorkspaceMode } from "./api/workbench";

export const WORKBENCH_MAIN_MODES = ["document"] as const;
export type WorkbenchMainMode = (typeof WORKBENCH_MAIN_MODES)[number];

export function resolveWorkbenchSurface(
  _mainMode: WorkbenchMainMode,
  workspaceMode: WorkspaceMode,
): WorkspaceMode {
  return workspaceMode;
}
