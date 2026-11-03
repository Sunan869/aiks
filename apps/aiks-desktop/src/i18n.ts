import i18next from "i18next";
import { initReactI18next } from "react-i18next";
import zhCN from "./locales/zh-CN.json";
import enUS from "./locales/en-US.json";

export type UiLanguage = "zh-CN" | "en-US";
export const LANGUAGE_STORAGE_KEY = "aiks.ui.language";

function resolveLanguage(input: string): UiLanguage {
  return input.toLowerCase().startsWith("en") ? "en-US" : "zh-CN";
}
function initialLanguage(): UiLanguage {
  try {
    const saved = window.localStorage.getItem(LANGUAGE_STORAGE_KEY);
    if (saved === "zh-CN" || saved === "en-US") return saved;
    return resolveLanguage(window.navigator.language || "zh-CN");
  } catch { return "zh-CN"; }
}

export const i18n = i18next.createInstance();
void i18n.use(initReactI18next).init({
  resources: { "zh-CN": { translation: zhCN }, "en-US": { translation: enUS } },
  lng: initialLanguage(),
  fallbackLng: "zh-CN",
  supportedLngs: ["zh-CN", "en-US"],
  interpolation: { escapeValue: false },
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
