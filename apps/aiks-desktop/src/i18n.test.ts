import { afterEach, describe, expect, it } from "vitest";
import { changeUiLanguage, getUiLanguage, i18n, resolveUiLanguage, tr, translateUiText } from "./i18n";
import en from "./locales/en-US.json";
import zh from "./locales/zh-CN.json";

afterEach(async () => { await i18n.changeLanguage("zh-CN"); });

describe("AIKS UI localization", () => {
  it("has exactly the same keys in zh-CN and en-US resources", () => {
    expect(Object.keys(en).sort()).toEqual(Object.keys(zh).sort());
  });

  it("uses explicit preference over system locale and safely falls back", () => {
    expect(resolveUiLanguage("zh-CN", "en-US")).toBe("zh-CN");
    expect(resolveUiLanguage("en-US", "zh-CN")).toBe("en-US");
    expect(resolveUiLanguage(null, "en-GB")).toBe("en-US");
    expect(resolveUiLanguage(null, "ja-JP")).toBe("zh-CN");
  });

  it("switches the actual i18next runtime, not just static navigation", async () => {
    await changeUiLanguage("en-US");
    expect(getUiLanguage()).toBe("en-US");
    expect(tr("处理中心")).toBe("Processing Center");
    expect(tr("立即生效，无需重启")).toBe("Applies immediately; no restart required");
    await changeUiLanguage("zh-CN");
    expect(tr("处理中心")).toBe("处理中心");
  });

  it("retranslates existing, authored runtime feedback and leaves unknown content alone", async () => {
    await i18n.changeLanguage("en-US");
    expect(translateUiText("作废预检失败，未执行删除")).toBe("Void preflight failed. Nothing was deleted.");
    expect(translateUiText("反馈加载失败：connection timeout")).toBe("Failed to load feedback: connection timeout");
    expect(translateUiText("同步提交完成：+12 新增，3 更新")).toBe("Sync submitted: +12 new, 3 updated.");
    expect(translateUiText("用中文写的用户原文，不应该翻译")).toBe("用中文写的用户原文，不应该翻译");
    await i18n.changeLanguage("zh-CN");
    expect(translateUiText("反馈加载失败：connection timeout")).toBe("反馈加载失败：connection timeout");
  });

  it("renders generated provider states and status bar text in English while retaining source values", async () => {
    await i18n.changeLanguage("en-US");
    expect(tr("待重启生效")).toBe("Restart required");
    expect(tr("未检测到本地会话数据")).toBe("No local session data detected");
    expect(tr("{{count}} 个数据源", { count: 12 })).toBe("12 data sources");
    expect(tr("最近扫描 {{time}}", { time: "10:42 AM" })).toBe("Last scanned 10:42 AM");
    expect(tr("总结一下最近讨论的向量模型部署方案")).toContain("deployment");
  });

  it("preserves the meaning of safety and manual-review boundaries", async () => {
    await i18n.changeLanguage("en-US");
    const warning = tr("所有关系先记录为建议，人工确认后才成为已确认的知识关联；原知识不会被自动删除或覆盖。跨 Provider 关系必须有同一已核实项目路径或显式人工归属，两个不同 Session 仅显示项目同名不能作为证据；缺失的来源不可确认。");
    expect(warning).toContain("human review");
    expect(warning).toContain("never automatically deleted or overwritten");
    expect(warning).toContain("project names alone");
  });
});
