import { describe, expect, it } from "vitest";
import workbenchHostSource from "../components/WorkbenchHost.tsx?raw";
import knowledgeWorkspaceSource from "../pages/KnowledgeWorkspacePage.tsx?raw";
import lifecycleSource from "../../src-tauri/src/lifecycle.rs?raw";
import workbenchCommandsSource from "../../src-tauri/src/workbench/commands.rs?raw";
import bridgePluginInstallerSource from "../../src-tauri/src/workbench/plugin.rs?raw";
import runtimeSource from "../../../../crates/aiks-core/src/runtime/mod.rs?raw";
import engineSource from "../../../../crates/aiks-core/src/engine/mod.rs?raw";

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
    expect(bridgePluginInstallerSource).toContain('config.insert("trust".to_string(), json!(true))');
    expect(bridgePluginInstallerSource).toContain('config.insert("petalDisabled".to_string(), json!(false))');
  });

  it("logs native child-webview diagnostics without relying on the bridge event channel", () => {
    expect(workbenchCommandsSource).toContain("on_page_load");
    expect(workbenchCommandsSource).toContain("on_document_title_changed");
    expect(workbenchCommandsSource).toContain("__AIKS_BRIDGE__");
    expect(workbenchCommandsSource).toContain("__AIKS_WORKBENCH_NONCE__");
    expect(workbenchCommandsSource).toContain("__TAURI_INTERNALS__");
    expect(workbenchCommandsSource).toContain("[WORKBENCH_DIAG]");
  });

  it("suspends the native child workbench while unified search is open", () => {
    expect(knowledgeWorkspaceSource).toContain("suspended={searchOpen}");
    expect(workbenchHostSource).toContain("suspended?: boolean");
    expect(workbenchHostSource).toContain("if (suspended)");
    expect(workbenchHostSource).toContain("getApi().hideWorkbench()");
  });

  it("records the real lifecycle trigger for automatic sync runs", () => {
    expect(engineSource).toContain("sync_and_enqueue_extraction_with_trigger");
    expect(lifecycleSource).toContain('sync_and_enqueue_extraction_with_trigger(opts, "startup")');
    expect(lifecycleSource).toContain('sync_and_enqueue_extraction_with_trigger(opts, "watcher")');
    expect(lifecycleSource).toContain('sync_and_enqueue_extraction_with_trigger(opts, "periodic")');
  });

  it("keeps Windows process liveness probes hidden from desktop users", () => {
    expect(runtimeSource).toMatch(
      /Command::new\("tasklist"\)[\s\S]{0,500}creation_flags\(CREATE_NO_WINDOW\)/,
    );
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
