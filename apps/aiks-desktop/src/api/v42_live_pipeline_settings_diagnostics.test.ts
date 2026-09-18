import { describe, expect, it } from "vitest";
import processingSource from "../pages/ProcessingPage.tsx?raw";
import diagnosticsPageSource from "../pages/DiagnosticsPage.tsx?raw";
import settingsPageSource from "../pages/SettingsPage.tsx?raw";
import commandsSource from "../../src-tauri/src/commands.rs?raw";
import libSource from "../../src-tauri/src/lib.rs?raw";

describe("V4.2 live pipeline/settings/diagnostics boundaries", () => {
  it("keeps Processing Center live while the page is visible", () => {
    expect(processingSource).toContain("visibilitychange");
    expect(processingSource).toMatch(/setInterval\([^,]+,\s*2000\)/s);
    expect(processingSource).toContain("document.visibilityState === \"visible\"");
    expect(processingSource).toContain("clearInterval");
  });

  it("uses V4.2 diagnostics copy and distinguishes unmounted workbench from bridge wait", () => {
    expect(diagnosticsPageSource).toContain("V4.2 知识工作台");
    expect(diagnosticsPageSource).toContain("未挂载");
    expect(diagnosticsPageSource).toContain("等待 Bridge");
    expect(diagnosticsPageSource).toContain("已连接");
    expect(diagnosticsPageSource).not.toContain("V4.1 知识工作台");
  });

  it("renders AI endpoint/model from backend settings instead of obsolete frontend defaults", () => {
    expect(settingsPageSource).not.toContain('useState("http://127.0.0.1:11434/v1")');
    expect(settingsPageSource).not.toContain('useState("qwen3")');
    expect(settingsPageSource).toContain("settings.ai_base_url");
    expect(settingsPageSource).toContain("settings.ai_model");
    expect(commandsSource).toContain("pub ai_base_url: String");
    expect(commandsSource).toContain("pub ai_model: String");
  });

  it("removes unsupported extraction pseudo-settings", () => {
    expect(settingsPageSource).not.toContain("ai_extract_tags");
    expect(settingsPageSource).not.toContain("ai_extract_problems");
    expect(settingsPageSource).not.toContain("ai_extract_decisions");
    expect(commandsSource).not.toContain("pub ai_extract_tags");
    expect(commandsSource).not.toContain("pub ai_extract_problems");
    expect(commandsSource).not.toContain("pub ai_extract_decisions");
  });

  it("loads settings from aiks.toml and wires real desktop controls", () => {
    expect(commandsSource).toContain("config_file_path()");
    expect(commandsSource).not.toContain('read_to_string(&settings_file)');
    expect(commandsSource).toContain("tauri_plugin_autostart");
    expect(commandsSource).toContain("test_ai_connection_with_settings");
    expect(libSource).toContain("close_to_tray");
  });

  it("tells users engine-owned settings require restart", () => {
    expect(settingsPageSource).toContain("重启 AIKS 后生效");
  });
});
