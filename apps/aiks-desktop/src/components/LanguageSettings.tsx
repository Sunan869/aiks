import { useTranslation } from "react-i18next";
import { RadixSelectField } from "./ui/RadixSelectField";
import { changeUiLanguage, getUiLanguage, tr, type UiLanguage } from "../i18n";

export function LanguageSettings() {
  useTranslation();
  return (
    <section aria-label={tr("界面语言")} className="mb-5 rounded-2xl border border-slate-200 bg-white p-4 shadow-sm dark:border-slate-800 dark:bg-slate-900">
      <h2 className="mb-2 text-sm font-bold tracking-tight text-slate-900 dark:text-slate-100">{tr("语言")}</h2>
      <div className="flex items-center justify-between gap-4 py-3">
        <div className="min-w-0">
          <div className="text-sm">{tr("界面语言")}</div>
          <div className="text-xs text-gray-500 dark:text-slate-400">{tr("立即生效，无需重启")}</div>
          <p className="mt-1 text-xs text-gray-500 dark:text-slate-400">{tr("修改界面语言不会影响模型、数据源或本地数据。")}</p>
        </div>
        <RadixSelectField label={tr("界面语言")} value={getUiLanguage()}
          onValueChange={value => void changeUiLanguage(value as UiLanguage)}
          className="min-w-[165px]"
          options={[{ value: "zh-CN", label: "简体中文" }, { value: "en-US", label: "English" }]}
        />
      </div>
    </section>
  );
}
