import { describe, expect, it } from "vitest";
import app from "../App.tsx?raw";
import settings from "../pages/SettingsPage.tsx?raw";
import storage from "../components/DataStorage.tsx?raw";
import diagnostics from "../pages/DiagnosticsPage.tsx?raw";
describe("desktop shell settings diagnostics layout", () => {
  it("embeds storage within the same settings page and removes large trailing spacing", () => {
    expect(app).toContain('case "settings": return <SettingsPage />;');
    expect(settings).toContain("<DataStorageSettingsSection />");
    expect(settings).not.toContain("pb-24");
    expect(storage).toContain('aria-label="数据存储位置"');
  });
  it("removes redundant top strip and preserves sidebar and footer", () => {
    expect(app).not.toContain("V4.2 Workbench</span>");
    expect(app).toContain("<Sidebar");
    expect(app).toContain("最近扫描");
  });
  it("groups health checks without removing diagnostic actions", () => {
    expect(diagnostics).toContain('UiSection title="本地组件检查"');
    expect(diagnostics).toContain('UiSection title="知识工作台与迁移"');
    expect(diagnostics).toContain('UiSection title="常用工具"');
    expect(diagnostics).toContain("getV41Diagnostics");
    expect(diagnostics).toContain('invoke("restart_siyuan")');
  });
});