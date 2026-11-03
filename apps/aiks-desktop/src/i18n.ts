import i18next from "i18next";
import { initReactI18next } from "react-i18next";
import zhCN from "./locales/zh-CN.json";
import enUS from "./locales/en-US.json";

export type UiLanguage = "zh-CN" | "en-US";
export const LANGUAGE_STORAGE_KEY = "aiks.ui.language";

function resolveLanguage(input: string): UiLanguage {
  return input.toLowerCase().startsWith("en") ? "en-US" : "zh-CN";
}
export function resolveUiLanguage(saved: string | null, system: string): UiLanguage {
  if (saved === "zh-CN" || saved === "en-US") return saved;
  return resolveLanguage(system || "zh-CN");
}

function initialLanguage(): UiLanguage {
  try {
    return resolveUiLanguage(window.localStorage.getItem(LANGUAGE_STORAGE_KEY), window.navigator.language || "zh-CN");
  } catch { return "zh-CN"; }
}

export const i18n = i18next.createInstance();
void i18n.use(initReactI18next).init({
  resources: { "zh-CN": { translation: zhCN }, "en-US": { translation: enUS } },
  lng: initialLanguage(),
  fallbackLng: "zh-CN",
  supportedLngs: ["zh-CN", "en-US"],
  interpolation: { escapeValue: false },
  keySeparator: false,
  nsSeparator: false,
  returnNull: false,
  react: { useSuspense: false },
});
i18n.on("languageChanged", lang => {
  if (typeof document !== "undefined") document.documentElement.lang = lang;
});
if (typeof document !== "undefined") document.documentElement.lang = i18n.language;

export function getUiLanguage(): UiLanguage { return resolveLanguage(i18n.language); }
export async function changeUiLanguage(lang: UiLanguage): Promise<void> {
  if (lang !== "zh-CN" && lang !== "en-US") return;
  await i18n.changeLanguage(lang);
  try { window.localStorage.setItem(LANGUAGE_STORAGE_KEY, lang); } catch { /* storage unavailable */ }
}
// Translate only render-time UI copy, never backend identifiers or persisted data.
export function tr(key: string, vars?: Record<string, unknown>): string {
  return i18n.t(key, { defaultValue: key, ...vars }) as string;
}

// Render-time localization for AIKS-authored status/feedback text only.
// Never run this on source sessions, user documents, model responses or provider data.
const feedbackPrefixes = [
  "读取数据源目录失败：","无法读取项目归属信息：","读取本地报告草稿失败：",
  "关系审核未完成：","未修改项目归属：","载入报告草稿失败：",
  "保存报告失败：","精选知识包生成失败：","复制失败：",
  "反馈加载失败：","整理历史读取失败：","整理记录状态更新失败：",
  "反馈保存失败：","关系加载失败：","关系人工审核历史读取失败：",
  "创建关系失败：","关系审核失败：","生成建议失败：",
  "无法关联现有审核记录：","归档生成的知识失败：",
  "恢复生成的知识失败：","操作失败：","SiYuan 发布结果：",
  "发布失败：","同步未完成：","保存失败：",
  "SiYuan 运行环境未就绪：","知识引擎文件不完整：",
  "知识引擎启动失败：","初始化失败：",
] as const;
const dynamicFeedbackKeys = [
  "知识引擎就绪 (v{{version}})",
  "已导入 {{count}} 条消息 · {{source}}",
  "同步提交完成：+{{added}} 新增，{{updated}} 更新",
  "扫描完成：+{{added}} 新增，{{updated}} 更新，{{failed}} 失败",
  "已重新提交 {{count}} 个任务（失败 + 未处理队列）",
  "作废清理完成，已移除 {{documents}} 个 SiYuan 文档；会话元数据保留",
  "作废清理完成，已移除 {{documents}} 个 SiYuan 文档",
  "已载入本地报告修订 v{{revision}}，再次保存只会追加新版本。",
  "已保存本地回顾版本 v{{revision}}。历史版本没有被覆盖。",
  "将清理 {{documents}} 个 SiYuan 文档、{{knowledge}} 条知识和关联索引。会话来源记录保留，之后可手动恢复并重新提取。检测到人工改动将停止清理。",
  "会话 #{{id}}：待处理 {{pending}}，运行中 {{running}}，知识条目 {{knowledge}}、知识切片 {{chunks}}、向量记录 {{vectors}}；SiYuan 会话文档 {{sessions}}、知识文档 {{documents}}。正式作废前会再次校验远端归属及人工修改；冲突时保持隔离，不删除用户改动。",
] as const;
function regexEscape(s: string): string {
  return s.replace(/[.*+?^\u0024{}()|[\]\\]/g, "\\$&");
}
const messagePatterns = dynamicFeedbackKeys.map(key => {
  const names: string[] = [];
  const parts = key.split(/(\{\{[a-z]+\}\})/g);
  const pattern = parts.map(part => {
    const variable = /^\{\{([a-z]+)\}\}$/.exec(part);
    if (variable) { names.push(variable[1]); return "([\\s\\S]+?)"; }
    return regexEscape(part);
  }).join("");
  return { key, names, regex: new RegExp("^" + pattern + "$") };
});

export function translateUiText(value: string): string {
  if (getUiLanguage() === "zh-CN") return value;
  const exact = tr(value);
  if (exact !== value) return exact;
  for (const { key, names, regex } of messagePatterns) {
    const match = regex.exec(value);
    if (!match) continue;
    const vars: Record<string, string> = {};
    names.forEach((name, index) => { vars[name] = match[index + 1]; });
    return tr(key, vars);
  }
  for (const prefix of feedbackPrefixes) {
    if (value.startsWith(prefix)) return tr(prefix) + value.slice(prefix.length);
  }
  return value;
}
