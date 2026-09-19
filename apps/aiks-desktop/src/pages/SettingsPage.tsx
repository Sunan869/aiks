import { useEffect, useState } from "react";
import { CheckCircle } from "lucide-react";
import { shouldUseMock } from "../api/client";

const LCO_MODEL = "LCO-Embedding/LCO-Embedding-Omni-3B-2605";
const BGE_MODEL = "bge-m3:latest";

interface EmbeddingSettings {
  embedding_enabled: boolean;
  embedding_base_url: string;
  embedding_model: string;
  embedding_dimensions: number;
}

interface Settings extends EmbeddingSettings {
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

interface EmbeddingProbeResponse {
  healthy: boolean;
  dimensions: number | null;
  message: string;
}

interface SemanticIndexProgress {
  total: number;
  completed: number;
  knowledge_total: number;
  session_total: number;
  succeeded: number;
  failed: number;
  embedded_chunks: number;
  current_kind: string | null;
  current_id: string | null;
}

interface SemanticIndexRebuildStats {
  total: number;
  completed: number;
  knowledge_total: number;
  session_total: number;
  succeeded: number;
  failed: number;
  embedded_chunks: number;
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
  embedding_enabled: false,
  embedding_base_url: "http://localhost:28090/v1",
  embedding_model: LCO_MODEL,
  embedding_dimensions: 2048,
};

export default function SettingsPage() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [saved, setSaved] = useState(false);
  const [saveError, setSaveError] = useState("");
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [aiHealthy, setAiHealthy] = useState<boolean | null>(null);
  const [aiTesting, setAiTesting] = useState(false);
  const [embeddingProbe, setEmbeddingProbe] = useState<EmbeddingProbeResponse | null>(null);
  const [embeddingTesting, setEmbeddingTesting] = useState(false);
  const [rebuilding, setRebuilding] = useState(false);
  const [rebuildProgress, setRebuildProgress] = useState<SemanticIndexProgress | null>(null);
  const [rebuildMessage, setRebuildMessage] = useState("");
  const isMock = shouldUseMock();

  useEffect(() => {
    if (isMock) {
      setSettings(MOCK_SETTINGS);
      return;
    }
    import("@tauri-apps/api/core").then(({ invoke }) => {
      Promise.all([
        invoke<Omit<Settings, keyof EmbeddingSettings>>("get_settings"),
        invoke<EmbeddingSettings>("get_embedding_settings"),
      ])
        .then(([base, embedding]) => setSettings({ ...base, ...embedding }))
        .catch(error => setSaveError(String(error)));
    });
  }, [isMock]);

  useEffect(() => {
    if (isMock) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void import("@tauri-apps/api/event")
      .then(({ listen }) => listen<SemanticIndexProgress>("semantic-index-progress", event => {
        if (!disposed) setRebuildProgress(event.payload);
      }))
      .then(callback => {
        if (disposed) callback();
        else unlisten = callback;
      });
    return () => {
      disposed = true;
      unlisten?.();
    };
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

  const testEmbeddingConnection = async () => {
    if (!settings) return;
    setEmbeddingProbe(null);
    setEmbeddingTesting(true);
    try {
      if (isMock) {
        setEmbeddingProbe({ healthy: true, dimensions: settings.embedding_dimensions, message: `连接正常 · ${settings.embedding_dimensions} 维` });
        return;
      }
      const { invoke } = await import("@tauri-apps/api/core");
      const result = await invoke<EmbeddingProbeResponse>("test_embedding_connection_with_settings", {
        baseUrl: settings.embedding_base_url,
        model: settings.embedding_model,
        dimensions: settings.embedding_dimensions,
      });
      setEmbeddingProbe(result);
    } catch (error) {
      setEmbeddingProbe({ healthy: false, dimensions: null, message: String(error) });
    } finally {
      setEmbeddingTesting(false);
    }
  };

  const rebuildSemanticIndex = async () => {
    if (!settings?.embedding_enabled) return;
    setRebuildMessage("");
    setRebuildProgress(null);
    setRebuilding(true);
    try {
      if (isMock) {
        const progress: SemanticIndexProgress = {
          total: 96,
          completed: 96,
          knowledge_total: 36,
          session_total: 60,
          succeeded: 96,
          failed: 0,
          embedded_chunks: 148,
          current_kind: null,
          current_id: null,
        };
        setRebuildProgress(progress);
        setRebuildMessage("语义索引重建完成");
        return;
      }
      const { invoke } = await import("@tauri-apps/api/core");
      const result = await invoke<SemanticIndexRebuildStats>("rebuild_semantic_index");
      setRebuildMessage(
        result.failed > 0
          ? `重建完成：成功 ${result.succeeded}，失败 ${result.failed}`
          : `重建完成：${result.succeeded} 条记录，${result.embedded_chunks} 个向量块`,
      );
    } catch (error) {
      setRebuildMessage(String(error));
    } finally {
      setRebuilding(false);
    }
  };

  const save = async (restart: boolean) => {
    if (!settings) return;
    setSaveError("");
    try {
      if (!isMock) {
        const { invoke } = await import("@tauri-apps/api/core");
        const appSettings = {
          startup: settings.startup,
          close_to_tray: settings.close_to_tray,
          sync_enabled: settings.sync_enabled,
          scan_interval_seconds: settings.scan_interval_seconds,
          include_thinking: settings.include_thinking,
          include_tool_calls: settings.include_tool_calls,
          max_tool_result_chars: settings.max_tool_result_chars,
          redact_secrets: settings.redact_secrets,
          ai_enabled: settings.ai_enabled,
          ai_auto_extract: settings.ai_auto_extract,
          ai_base_url: settings.ai_base_url,
          ai_model: settings.ai_model,
        };
        const embeddingSettings: EmbeddingSettings = {
          embedding_enabled: settings.embedding_enabled,
          embedding_base_url: settings.embedding_base_url,
          embedding_model: settings.embedding_model,
          embedding_dimensions: settings.embedding_dimensions,
        };
        await invoke("save_settings", { settings: appSettings });
        await invoke("save_embedding_settings", { settings: embeddingSettings });
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

  const embeddingPreset = settings?.embedding_model === LCO_MODEL
    ? "lco"
    : settings?.embedding_model === BGE_MODEL
      ? "bge"
      : "custom";

  const applyEmbeddingPreset = (preset: string) => {
    if (preset === "lco") {
      setSettings(current => current ? {
        ...current,
        embedding_model: LCO_MODEL,
        embedding_dimensions: 2048,
      } : current);
    } else if (preset === "bge") {
      setSettings(current => current ? {
        ...current,
        embedding_model: BGE_MODEL,
        embedding_dimensions: 1024,
      } : current);
    }
    setEmbeddingProbe(null);
  };

  if (!settings) return <div className="p-6 text-gray-400 text-sm">加载中...</div>;

  return (
    <div className="p-6 max-w-xl">
      <div className="mb-6">
        <h1 className="text-xl font-semibold">设置</h1>
        <p className="mt-1 text-xs text-gray-400">桌面行为保存后立即生效；AI、同步、内容和语义搜索配置将在重启 AIKS 后生效。</p>
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

      <Section title="语义搜索">
        <Toggle
          label="启用语义搜索"
          desc="关闭时继续使用关键词检索；开启后融合关键词与向量召回"
          value={settings.embedding_enabled}
          onChange={value => {
            update("embedding_enabled", value);
            setEmbeddingProbe(null);
          }}
        />
        <div className="py-3 space-y-3">
          <div>
            <div className="text-xs text-gray-400 mb-1">向量模型预设</div>
            <select
              value={embeddingPreset}
              onChange={event => applyEmbeddingPreset(event.target.value)}
              className="w-full text-sm border border-gray-300 dark:border-gray-600 rounded px-2 py-1.5 bg-white dark:bg-gray-700"
            >
              <option value="lco">LCO Omni 3B 2605（推荐 · 2048 维）</option>
              <option value="bge">BGE-M3（轻量 · 1024 维）</option>
              <option value="custom">自定义</option>
            </select>
            <div className="mt-1 text-[11px] text-gray-400">AIKS 当前只提交文本；LCO 的图片、音频、视频能力不会被调用。</div>
          </div>
          <div>
            <div className="text-xs text-gray-400 mb-1">Embedding 服务地址</div>
            <input
              value={settings.embedding_base_url}
              onChange={event => {
                update("embedding_base_url", event.target.value);
                setEmbeddingProbe(null);
              }}
              placeholder="例如 http://127.0.0.1:28090/v1"
              className="w-full text-xs border border-gray-300 dark:border-gray-600 rounded px-2 py-1.5 bg-white dark:bg-gray-700"
            />
          </div>
          <div>
            <div className="text-xs text-gray-400 mb-1">Embedding 模型</div>
            <input
              value={settings.embedding_model}
              onChange={event => {
                update("embedding_model", event.target.value);
                setEmbeddingProbe(null);
              }}
              className="w-full text-xs border border-gray-300 dark:border-gray-600 rounded px-2 py-1.5 bg-white dark:bg-gray-700"
            />
          </div>
          <div>
            <div className="text-xs text-gray-400 mb-1">向量维度</div>
            <input
              type="number"
              min={1}
              value={settings.embedding_dimensions}
              onChange={event => {
                update("embedding_dimensions", Number(event.target.value));
                setEmbeddingProbe(null);
              }}
              className="w-full text-xs border border-gray-300 dark:border-gray-600 rounded px-2 py-1.5 bg-white dark:bg-gray-700"
            />
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <button
              type="button"
              onClick={() => void testEmbeddingConnection()}
              disabled={embeddingTesting}
              className="text-xs px-3 py-1.5 border border-gray-300 dark:border-gray-600 rounded hover:bg-gray-50 dark:hover:bg-gray-700 disabled:opacity-50"
            >
              {embeddingTesting ? "测试中..." : "测试 Embedding 连接"}
            </button>
            <button
              type="button"
              onClick={() => void rebuildSemanticIndex()}
              disabled={!settings.embedding_enabled || rebuilding}
              className="text-xs px-3 py-1.5 border border-blue-300 text-blue-600 dark:border-blue-700 dark:text-blue-300 rounded hover:bg-blue-50 dark:hover:bg-blue-900/20 disabled:opacity-50"
            >
              {rebuilding ? "重建中..." : "重建语义索引"}
            </button>
            {embeddingProbe && (
              <span className={`text-xs ${embeddingProbe.healthy ? "text-green-600" : "text-red-500"}`}>
                {embeddingProbe.healthy ? "✓" : "✕"} {embeddingProbe.message}
              </span>
            )}
          </div>
          {rebuildProgress && (
            <div className="rounded bg-gray-50 px-3 py-2 text-xs text-gray-600 dark:bg-gray-900/40 dark:text-gray-300">
              已向量化 {rebuildProgress.completed}/{rebuildProgress.total} 条记录
              <span className="ml-2 text-gray-400">知识 {rebuildProgress.knowledge_total} · 对话 {rebuildProgress.session_total} · 向量块 {rebuildProgress.embedded_chunks}</span>
            </div>
          )}
          {rebuildMessage && <div className="text-xs text-gray-500 dark:text-gray-400">{rebuildMessage}</div>}
          <div className="text-[11px] leading-5 text-gray-400">
            配置保存并重启后搜索服务才会切换模型；历史数据需执行一次“重建语义索引”。重建期间关键词检索仍可使用。
          </div>
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
        AI 服务、模型、同步周期、内容规则和语义搜索配置保存后需重启 AIKS 后生效；开机启动和关闭驻留设置立即生效。
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
