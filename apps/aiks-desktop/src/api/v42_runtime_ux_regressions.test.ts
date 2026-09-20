import { describe, expect, it } from "vitest";
import appSource from "../App.tsx?raw";
import diagnosticsSource from "../pages/DiagnosticsPage.tsx?raw";
import settingsSource from "../pages/SettingsPage.tsx?raw";
import sourcesSource from "../pages/SourcesPage.tsx?raw";
import apiInterfaceSource from "./index.ts?raw";
import tauriApiSource from "./tauri.ts?raw";
import embeddingCommandsSource from "../../src-tauri/src/embedding_commands.rs?raw";
import desktopLibSource from "../../src-tauri/src/lib.rs?raw";
import workbenchCommandsSource from "../../src-tauri/src/workbench/commands.rs?raw";
import configSource from "../../../../crates/aiks-core/src/config/mod.rs?raw";

describe("V4.2 real-machine UX regressions", () => {
  it("persists embedding config before optional desktop integration and uses one reliable config writer", () => {
    expect(configSource).toContain("pub fn write_file");
    expect(embeddingCommandsSource).toContain("config.write_file(&path)");
    expect(embeddingCommandsSource).not.toContain("aiks.toml.embedding.tmp");
    expect(settingsSource).toContain("saveWarning");

    const embeddingSave = settingsSource.indexOf('invoke("save_embedding_settings"');
    const appSave = settingsSource.indexOf('invoke<SaveSettingsResult>("save_settings"');
    expect(embeddingSave).toBeGreaterThan(-1);
    expect(appSave).toBeGreaterThan(embeddingSave);
  });

  it("lets sources, settings, and diagnostics use the available main-pane width", () => {
    for (const source of [sourcesSource, settingsSource, diagnosticsSource]) {
      expect(source).toContain("w-full min-w-0");
      expect(source).not.toMatch(/max-w-(?:xl|2xl)/);
    }
  });

  it("reloads the SiYuan child workbench whenever the Knowledge nav item is selected", () => {
    expect(apiInterfaceSource).toContain("reloadWorkbench(): Promise<void>");
    expect(tauriApiSource).toContain('invoke("reload_workbench")');
    expect(workbenchCommandsSource).toContain("pub fn reload_workbench");
    expect(desktopLibSource).toContain("workbench::commands::reload_workbench");
    expect(appSource).toContain('if (page === "knowledge")');
    expect(appSource).toContain("getApi().reloadWorkbench()");
  });
});
