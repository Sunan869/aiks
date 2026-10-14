import { describe, expect, it } from "vitest";
import ui from "./pages/ProcessingPage.tsx?raw";

describe("S1.2 task diagnostics privacy and preview boundary", () => {
  it("uses allowlisted aggregate-only diagnostics without serializing task rows", () => {
    expect(ui).toContain('import { createSafeTaskDiagnosticJson } from "../task-diagnostics";');
    expect(ui).toContain("createSafeTaskDiagnosticJson(taskStats, tasks)");
    expect(ui).not.toContain('JSON.stringify({\n    schema: "aiks-task-diagnostics-v1"');
    expect(ui).toContain("不导出 Session/任务 ID、标题、文件路径、原文、错误文本或密钥");
  });

  it("requires explicit preview before save and freezes the inspected snapshot", () => {
    expect(ui).toContain('const [diagnosticJson, setDiagnosticJson] = useState<string | null>(null)');
    expect(ui).toContain("setDiagnosticJson(createSafeTaskDiagnosticJson(taskStats, tasks))");
    expect(ui).toContain("showDiagnosticPreview && diagnosticJson !== null");
    expect(ui).toContain("保存审核后的 JSON");
    expect(ui).toContain("更新预览快照");
    expect(ui).toContain('link.download = "aiks-task-diagnostics-v2.json"');
    expect(ui).toContain("setShowDiagnosticPreview(false);");
    expect(ui).toContain("setDiagnosticJson(null);");
    expect(ui).toContain("new Blob([diagnosticJson]");
  });
});
