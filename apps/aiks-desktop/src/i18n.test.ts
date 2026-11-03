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

  it("localizes native startup progress and keeps detailed diagnostics intact", async () => {
    await i18n.changeLanguage("en-US");
    expect(translateUiText("正在验证知识引擎...")).toBe("Validating the knowledge engine…");
    expect(translateUiText("知识引擎就绪 (v3.8.3)")).toBe("Knowledge engine ready (v3.8.3)");
    expect(translateUiText("知识引擎启动失败：OS error 2")).toBe("Failed to start knowledge engine: OS error 2");
    expect(translateUiText("some unknown backend error")).toBe("some unknown backend error");
  });

  it("localizes source names, controlled categories and imported-message feedback", async () => {
    await i18n.changeLanguage("en-US");
    expect(tr("豆包")).toBe("Doubao");
    expect(tr("腾讯元宝")).toBe("Tencent Yuanbao");
    expect(tr("千问")).toBe("Qwen");
    expect(tr("架构设计")).toBe("Architecture");
    expect(translateUiText("已导入 21 条消息 · 豆包")).toBe("Imported 21 messages · 豆包");
    expect(tr("user-project-123")).toBe("user-project-123");
  });

  it("keeps data migration and human review confirmations accurate in English", async () => {
    await i18n.changeLanguage("en-US");
    const migration = tr("将 AIKS 数据从：\n{{from}}\n\n迁移到：\n{{to}}\n\nAIKS 会立即重启，并在打开数据库和 SiYuan 之前完成复制与校验。原目录会暂时保留作为安全备份。是否继续？", { from: "C:\\\\AIKS", to: "D:\\\\AIKS" });
    expect(migration).toContain("before opening the database or SiYuan");
    expect(migration).toContain("C:\\\\AIKS");
    expect(migration).toContain("D:\\\\AIKS");
    expect(tr("只在 AIKS 本地归档这条审核后创建的独立知识？原知识与 SiYuan 文档均会保留。")).toContain("retained");
    expect(tr("将 {{session}} 归入「{{project}}」？\n仅更改 AIKS 本地记忆归组，不修改 Provider 原始文件、Session 原始路径或 SiYuan 文档。", {
      session: "Session A", project: "Project B",
    })).toContain("Provider files");
  });

  it("renders composed review and diagnostics notices without translating user evidence", async () => {
    await i18n.changeLanguage("en-US");
    expect(translateUiText("待人工核对：存在修正关系，需复核两份证据与适用范围"))
      .toContain("Needs manual review: A correction relation exists");
    expect(translateUiText("已有人确认关联：存在修正关系，需复核两份证据与适用范围（不等于事实已被证明）"))
      .toContain("not proof of factual correctness");
    expect(translateUiText("以下检测未完成：本地环境、AI 连接。为保护敏感配置，不显示原始错误详情。"))
      .toContain("Local environment, AI connection");
    expect(translateUiText("人工写入的原始知识证据，不能翻译")).toBe("人工写入的原始知识证据，不能翻译");
  });

  it("preserves the meaning of safety and manual-review boundaries", async () => {
    await i18n.changeLanguage("en-US");
    const warning = tr("所有关系先记录为建议，人工确认后才成为已确认的知识关联；原知识不会被自动删除或覆盖。跨 Provider 关系必须有同一已核实项目路径或显式人工归属，两个不同 Session 仅显示项目同名不能作为证据；缺失的来源不可确认。");
    expect(warning).toContain("human review");
    expect(warning).toContain("never automatically deleted or overwritten");
    expect(warning).toContain("project names alone");
  });
});
