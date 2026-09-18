import { useEffect, useState } from "react";
import { CheckCircle } from "lucide-react";
import { shouldUseMock } from "../api/client";

interface Settings {
  startup: boolean;
  close_to_tray: boolean;
  sync_enabled: boolean;
  scan_interval_seconds: number;
  include_thinking: boolean;
  include_tool_calls: boolean;
  max_tool_result_chars: number;
  redact_secrets: boolean;
  ai_enabled: boolean;
  ai_auto_extract: boolean;
  ai_base_url: string;
  ai_model: string;
}

const MOCK_SETTINGS: Settings = {
  startup: true,
  close_to_tray: true,
  sync_enabled: true,
  scan_interval_seconds: 300,
  include_thinking: false,
  include_tool_calls: true,
  max_tool_result_chars: 10000,
  redact_secrets: true,
  ai_enabled: true,
  ai_auto_extract: true,
  ai_base_url: "http://localhost:11434/v1",
  ai_model: "Qwen3.8-27B",
};

export default function SettingsPage() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [saved, setSaved] = useState(false);
  const [saveError, setSaveError] = useState("");
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [aiHealthy, setAiHealthy] = useState<boolean | null>(null);
  const [aiTesting, setAiTesting] = useState(false);
  const isMock = shouldUseMock();

  useEffect(() => {
    if (isMock) {
      setSettings(MOCK_SETTINGS);
      return;
    }
    import("@tauri-apps/api/core").then(({ invoke }) => {
      invoke<Settings>("get_settings").then(setSettings).catch(error => setSaveError(String(error)));
    });
  }, [isMock]);

  const testAiConnection = async () => {
    if (!settings) return;
    setAiHealthy(null);
    setAiTesting(true);
    try {
      if (isMock) {
        setAiHealthy(true);
        return;
      }
      const { invoke } = await import("@tauri-apps/api/core");
      const ok = await invoke<boolean>("test_ai_connection_with_settings", {
        baseUrl: settings.ai_base_url,
        model: settings.ai_model,
      });
      setAiHealthy(ok);
    } catch {
      setAiHealthy(false);
    } finally {
      setAiTesting(false);
    }
  };

  const save = async (restart: boolean) => {
    if (!settings) return;
    setSaveError("");
    try {
      if (!isMock) {
        const { invoke } = await import("@tauri-apps/api/core");
        await invoke("save_settings", { settings });
        if (restart) {
          await invoke("restart_app");
          return;
        }
      }
      setSaved(true);
      window.setTimeout(() => setSaved(false), 2000);
    } catch (error) {
      setSaveError(String(error));
    }
  };

  const update = (key: keyof Settings, value: boolean | number | string) => {
    setSettings(current => current ? { ...current, [key]: value } : current);
  };

  if (!settings) return <div className="p-6 text-gray-400 text-sm">加载中...</div>;

  return (
    <div className="p-6 max-w-xl">
      <div className="mb-6">
        <h1 className="text-xl font-semibold">设置</h1>
        <p className="mt-1 text-xs text-gray-400">桌面行为保存后立即生效；AI、同步和内容配置将在重启 AIKS 后生效。</p>
      </div>

      <Section title="常规">
        <Toggle label="开机自动启动" desc="登录后自动在后台运行" value={settings.startup} onChange={value => update("startup", value)} />
        <Toggle label="关闭窗口后驻留后台" desc="关闭主窗口时保留托盘与后台同步" value={settings.close_to_tray} onChange={value => update("close_to_tray", value)} />
      </Section>

      <Section title="同步">
        <Toggle label="实时自动同步" desc="监测文件变化并自动同步" value={settings.sync_enabled} onChange={value => update("sync_enabled", value)} />
        <div className="flex items-center justify-between py-3">
          <div>
            <div className="text-sm">定时扫描间隔</div>
            <div className="text-xs text-gray-400">重启 AIKS 后生效</div>
          </div>
          <select
            className="text-sm border border-gray-300 dark:border-gray-600 rounded px-2 py-1 bg-white dark:bg-gray-700"
            value={settings.scan_interval_seconds / 60}
            onChange={event => update("scan_interval_seconds", +event.target.value * 60)}
          >
            {[1, 5, 10, 15, 30].map(minutes => <option key={minutes} value={minutes}>{minutes} 分钟</option>)}
          </select>
        </div>
      </Section>

      <Section title="AI 智能整理">
        <Toggle label="启用智能整理" desc="使用配置的 AI 服务自动提炼知识" value={settings.ai_enabled} onChange={value => update("ai_enabled", value)} />
        <Toggle label="自动整理新会话" desc="Raw Session 同步成功后自动进入处理队列" value={settings.ai_auto_extract} onChange={value => update("ai_auto_extract", value)} />
        <div className="py-3">
          <div className="text-xs text-gray-400">当前模型</div>
          <div className="mt-1 text-sm font-medium text-gray-700 dark:text-gray-300">{settings.ai_model}</div>
        </div>
      </Section>

      <Section title="内容">
        <Toggle label="保存思考过程" desc="同步源数据中的 thinking 内容" value={settings.include_thinking} onChange={value => update("include_thinking", value)} />
        <Toggle label="保存 Tool Call" desc="记录 AI 执行的工具调用" value={settings.include_tool_calls} onChange={value => update("include_tool_calls", value)} />
        <Toggle label="Secret 脱敏" desc="自动过滤 API Key、Token 等敏感信息" value={settings.redact_secrets} onChange={value => update("redact_secrets", value)} />
      </Section>

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
                value={settings.ai_base_url}
                onChange={event => update("ai_base_url", event.target.value)}
                className="w-full text-xs border border-gray-300 dark:border-gray-600 rounded px-2 py-1 bg-white dark:bg-gray-700"
              />
            </div>
            <div>
              <div className="text-xs text-gray-400 mb-1">AI 模型</div>
              <input
                value={settings.ai_model}
                onChange={event => update("ai_model", event.target.value)}
                className="w-full text-xs border border-gray-300 dark:border-gray-600 rounded px-2 py-1 bg-white dark:bg-gray-700"
              />
            </div>
            <div>
              <div className="text-xs text-gray-400 mb-1">Tool Result 最大字符数</div>
              <input
                type="number"
                min={1000}
                step={1000}
                value={settings.max_tool_result_chars}
                onChange={event => update("max_tool_result_chars", Number(event.target.value))}
                className="w-full text-xs border border-gray-300 dark:border-gray-600 rounded px-2 py-1 bg-white dark:bg-gray-700"
              />
            </div>
            <div className="flex items-center gap-2">
              <button
                onClick={testAiConnection}
                disabled={aiTesting}
                className="text-xs px-3 py-1 border border-gray-300 dark:border-gray-600 rounded hover:bg-gray-50 dark:hover:bg-gray-700 disabled:opacity-50"
              >
                {aiTesting ? "测试中..." : "测试当前 AI 配置"}
              </button>
              {aiHealthy !== null && (
                <span className={`text-xs ${aiHealthy ? "text-green-600" : "text-red-500"}`}>
                  {aiHealthy ? "✓ 连接正常" : "✕ 连接失败"}
                </span>
              )}
            </div>
          </div>
        )}
      </div>

      <div className="rounded-lg bg-blue-50 px-3 py-2 text-xs text-blue-600 dark:bg-blue-900/20 dark:text-blue-300">
        AI 服务、模型、同步周期和内容规则保存后需重启 AIKS 后生效；开机启动和关闭驻留设置立即生效。
      </div>
      {saveError && <div className="mt-3 text-xs text-red-500">{saveError}</div>}

      <div className="mt-4 flex items-center gap-2">
        <button
          onClick={() => void save(false)}
          className="px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded-lg text-sm transition-colors"
        >
          {saved ? <span className="flex items-center gap-1"><CheckCircle className="w-4 h-4" />已保存</span> : "保存设置"}
        </button>
        {!isMock && (
          <button
            onClick={() => void save(true)}
            className="px-4 py-2 border border-blue-300 text-blue-600 rounded-lg text-sm hover:bg-blue-50 dark:border-blue-700 dark:text-blue-300 dark:hover:bg-blue-900/20"
          >
            保存并重启
          </button>
        )}
      </div>
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

function Toggle({ label, desc, value, onChange }: { label: string; desc: string; value: boolean; onChange: (value: boolean) => void }) {
  return (
    <div className="flex items-center justify-between py-3">
      <div>
        <div className="text-sm">{label}</div>
        {desc && <div className="text-xs text-gray-400">{desc}</div>}
      </div>
      <button
        role="switch"
        aria-checked={value}
        onClick={() => onChange(!value)}
        className={`relative shrink-0 w-10 h-5 rounded-full transition-colors ${value ? "bg-blue-600" : "bg-gray-300 dark:bg-gray-600"}`}
      >
        <span className={`absolute left-0.5 top-0.5 w-4 h-4 bg-white rounded-full shadow transition-transform ${value ? "translate-x-5" : "translate-x-0"}`} />
      </button>
    </div>
  );
}
