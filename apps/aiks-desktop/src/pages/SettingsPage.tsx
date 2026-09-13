import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { CheckCircle } from "lucide-react";

interface Settings {
  startup: boolean;
  sync_enabled: boolean;
  scan_interval_seconds: number;
  include_thinking: boolean;
  include_tool_calls: boolean;
  max_tool_result_chars: number;
  redact_secrets: boolean;
  ai_enabled: boolean;
  ai_auto_extract: boolean;
  ai_extract_tags: boolean;
  ai_extract_problems: boolean;
  ai_extract_decisions: boolean;
}

export default function SettingsPage() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [saved, setSaved] = useState(false);
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [aiBaseUrl, setAiBaseUrl] = useState("http://10.10.23.16:18000/v1");
  const [aiModel, setAiModel] = useState("Qwen3.8-27B");

  useEffect(() => {
    invoke<Settings>("get_settings").then(setSettings).catch(console.error);
  }, []);

  const save = async () => {
    if (!settings) return;
    await invoke("save_settings", { settings });
    setSaved(true);
    setTimeout(() => setSaved(false), 2000);
  };

  const update = (key: keyof Settings, value: boolean | number) => {
    setSettings(s => s ? { ...s, [key]: value } : s);
  };

  if (!settings) return <div className="p-6 text-gray-400 text-sm">加载中...</div>;

  return (
    <div className="p-6 max-w-xl">
      <div className="mb-6">
        <h1 className="text-xl font-semibold">设置</h1>
      </div>

      {/* General */}
      <Section title="常规">
        <Toggle label="开机自动启动" desc="登录后自动在后台运行" value={settings.startup} onChange={v => update("startup", v)} />
        <Toggle label="关闭窗口后驻留后台" desc="关闭窗口时保持后台同步" value={true} onChange={() => {}} />
      </Section>

      {/* Sync */}
      <Section title="同步">
        <Toggle label="实时自动同步" desc="监测文件变化并自动同步" value={settings.sync_enabled} onChange={v => update("sync_enabled", v)} />
        <div className="flex items-center justify-between py-3">
          <div>
            <div className="text-sm">定时扫描间隔</div>
            <div className="text-xs text-gray-400">分钟</div>
          </div>
          <select
            className="text-sm border border-gray-300 dark:border-gray-600 rounded px-2 py-1 bg-white dark:bg-gray-700"
            value={settings.scan_interval_seconds / 60}
            onChange={e => update("scan_interval_seconds", +e.target.value * 60)}
          >
            {[1,5,10,15,30].map(m => <option key={m} value={m}>{m} 分钟</option>)}
          </select>
        </div>
      </Section>

      {/* AI */}
      <Section title="AI 智能整理">
        <Toggle label="启用智能整理" desc="使用公司内部 AI 自动提炼知识" value={settings.ai_enabled} onChange={v => update("ai_enabled", v)} />
        <Toggle label="自动整理新会话" desc="10 分钟无变化后自动整理" value={settings.ai_auto_extract} onChange={v => update("ai_auto_extract", v)} />
        <Toggle label="自动生成标签" desc="" value={settings.ai_extract_tags} onChange={v => update("ai_extract_tags", v)} />
        <Toggle label="提取问题与解决方案" desc="" value={settings.ai_extract_problems} onChange={v => update("ai_extract_problems", v)} />
        <Toggle label="提取设计决策" desc="" value={settings.ai_extract_decisions} onChange={v => update("ai_extract_decisions", v)} />
        <div className="py-2">
          <div className="text-sm mb-1 text-gray-500">AI 模型</div>
          <div className="text-sm font-medium text-gray-700 dark:text-gray-300">公司内部 AI</div>
        </div>
      </Section>

      {/* Content */}
      <Section title="内容">
        <Toggle label="保存 Tool Call" desc="记录 AI 执行的工具调用" value={settings.include_tool_calls} onChange={v => update("include_tool_calls", v)} />
        <Toggle label="Secret 脱敏" desc="自动过滤 API Key、Token 等敏感信息" value={settings.redact_secrets} onChange={v => update("redact_secrets", v)} />
      </Section>

      {/* Advanced (collapsed) */}
      <div className="mb-4">
        <button
          onClick={() => setShowAdvanced(!showAdvanced)}
          className="text-xs text-gray-400 hover:text-gray-600 flex items-center gap-1"
        >
          {showAdvanced ? "▾" : "▸"} 高级设置
        </button>
        {showAdvanced && (
          <div className="mt-3 bg-gray-50 dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 p-4 space-y-3 text-sm">
            <div>
              <div className="text-xs text-gray-400 mb-1">知识引擎</div>
              <div className="text-gray-600 dark:text-gray-300">SiYuan 3.8.3 (内置)</div>
            </div>
            <div>
              <div className="text-xs text-gray-400 mb-1">AI 服务地址</div>
              <input
                value={aiBaseUrl}
                onChange={e => setAiBaseUrl(e.target.value)}
                className="w-full text-xs border border-gray-300 dark:border-gray-600 rounded px-2 py-1 bg-white dark:bg-gray-700"
              />
            </div>
            <div>
              <div className="text-xs text-gray-400 mb-1">AI 模型</div>
              <input
                value={aiModel}
                onChange={e => setAiModel(e.target.value)}
                className="w-full text-xs border border-gray-300 dark:border-gray-600 rounded px-2 py-1 bg-white dark:bg-gray-700"
              />
            </div>
          </div>
        )}
      </div>

      <button
        onClick={save}
        className="px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded-lg text-sm transition-colors"
      >
        {saved ? <span className="flex items-center gap-1"><CheckCircle className="w-4 h-4" />已保存</span> : "保存设置"}
      </button>
    </div>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="mb-5">
      <div className="text-xs font-semibold text-gray-400 uppercase tracking-wider mb-2">{title}</div>
      <div className="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 divide-y divide-gray-100 dark:divide-gray-700 px-4">
        {children}
      </div>
    </div>
  );
}

function Toggle({ label, desc, value, onChange }: { label: string; desc: string; value: boolean; onChange: (v: boolean) => void }) {
  return (
    <div className="flex items-center justify-between py-3">
      <div>
        <div className="text-sm">{label}</div>
        {desc && <div className="text-xs text-gray-400">{desc}</div>}
      </div>
      <button
        role="switch" aria-checked={value}
        onClick={() => onChange(!value)}
        className={`relative w-9 h-5 rounded-full transition-colors ${value ? "bg-blue-600" : "bg-gray-300 dark:bg-gray-600"}`}
      >
        <span className={`absolute top-0.5 w-4 h-4 bg-white rounded-full shadow transition-transform ${value ? "translate-x-4" : "translate-x-0.5"}`} />
      </button>
    </div>
  );
}
