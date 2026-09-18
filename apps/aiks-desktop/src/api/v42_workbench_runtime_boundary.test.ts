import { describe, expect, it } from "vitest";
import workbenchHostSource from "../components/WorkbenchHost.tsx?raw";
import knowledgeWorkspaceSource from "../pages/KnowledgeWorkspacePage.tsx?raw";

type Capability = {
  identifier?: string;
  local?: boolean;
  webviews?: string[];
  remote?: { urls?: string[] };
  permissions?: string[];
};

const capabilityFiles = import.meta.glob("../../src-tauri/capabilities/*.json", {
  eager: true,
  import: "default",
}) as Record<string, Capability>;

const powerShellScripts = import.meta.glob("../../../../scripts/*.ps1", {
  eager: true,
  import: "default",
  query: "?raw",
}) as Record<string, string>;

describe("V4.2 embedded workbench runtime boundaries", () => {
  it("grants the remote SiYuan workbench only the event emit IPC permission", () => {
    const capability = Object.entries(capabilityFiles)
      .find(([path]) => path.endsWith("/siyuan-workbench-bridge.json"))?.[1];

    expect(capability).toBeDefined();
    if (!capability) return;

    expect(capability.identifier).toBe("siyuan-workbench-bridge");
    expect(capability.local).toBe(false);
    expect(capability.webviews).toEqual(["siyuan-workbench", "knowledge"]);
    expect(capability.remote?.urls).toEqual([
      "http://127.0.0.1:*/*",
      "http://localhost:*/*",
    ]);
    expect(capability.permissions).toEqual(["core:event:allow-emit"]);
  });

  it("suspends the native child workbench while unified search is open", () => {
    expect(knowledgeWorkspaceSource).toContain("suspended={searchOpen}");
    expect(workbenchHostSource).toContain("suspended?: boolean");
    expect(workbenchHostSource).toContain("if (suspended)");
    expect(workbenchHostSource).toContain("getApi().hideWorkbench()");
  });

  it("ships a guarded PowerShell reset that removes only the resolved AIKS data root", () => {
    const resetScript = Object.entries(powerShellScripts)
      .find(([path]) => path.endsWith("/reset-aiks-data.ps1"))?.[1];

    expect(resetScript).toBeDefined();
    if (!resetScript) return;

    expect(resetScript).toContain("AIKS_DATA_DIR");
    expect(resetScript).toContain("data-root.txt");
    expect(resetScript).toContain("AIKnowledgeSync");
    expect(resetScript).toContain("GetPathRoot");
    expect(resetScript).toContain("SiYuan-Kernel");
    expect(resetScript).toContain('Read-Host "Type RESET to continue"');
    expect(resetScript).toContain("Remove-Item -LiteralPath $dataRoot -Recurse -Force");
  });
});
