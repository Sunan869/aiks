import { describe, expect, it } from "vitest";
import workbenchHostSource from "../components/WorkbenchHost.tsx?raw";
import knowledgeWorkspaceSource from "../pages/KnowledgeWorkspacePage.tsx?raw";
import lifecycleSource from "../../src-tauri/src/lifecycle.rs?raw";
import bridgePluginInstallerSource from "../../src-tauri/src/workbench/plugin.rs?raw";

type Capability = {
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
  it("keeps the loopback SiYuan workbench capability restricted to event emission", () => {
    const capability = Object.entries(capabilityFiles)
      .find(([path]) => path.endsWith("/siyuan-workbench.json"))?.[1];

    expect(capability).toBeDefined();
    if (!capability) return;

    expect(capability.webviews).toEqual(["siyuan-workbench", "knowledge"]);
    expect(capability.remote?.urls).toEqual([
      "http://127.0.0.1:*/*",
      "http://localhost:*/*",
    ]);
    expect(capability.permissions).toEqual(["core:event:allow-emit"]);
  });

  it("automatically activates the bundled AIKS bridge before the workbench can mount", () => {
    expect(lifecycleSource).toContain("ensure_bridge_plugin_enabled(&base_url).await");
    expect(bridgePluginInstallerSource).toContain("/api/system/getConf");
    expect(bridgePluginInstallerSource).toContain("/api/setting/setBazaar");
    expect(bridgePluginInstallerSource).toContain("/api/petal/setPetalEnabled");
    expect(bridgePluginInstallerSource).toContain('"packageName": "aiks-bridge"');
    expect(bridgePluginInstallerSource).toContain('"trust": true');
    expect(bridgePluginInstallerSource).toContain('"petalDisabled": false');
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
