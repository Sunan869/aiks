import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { CheckCircle, XCircle, RefreshCw, Cpu, FileText } from "lucide-react";

interface DoctorCheck { name: string; ok: boolean; message: string }
interface DoctorResult { checks: DoctorCheck[]; all_ok: boolean }

export default function DiagnosticsPage() {
  const [doctor, setDoctor] = useState<DoctorResult | null>(null);
  const [aiOk, setAiOk] = useState<boolean | null>(null);
  const [loading, setLoading] = useState(false);

  const runDiag = async () => {
    setLoading(true);
    try {
      const [d, ai] = await Promise.all([
        invoke<DoctorResult>("get_doctor"),
        invoke<boolean>("test_ai_connection"),
      ]);
      setDoctor(d);
      setAiOk(ai);
    } catch (e) {
      console.error(e);
    } finally { setLoading(false); }
  };

  useEffect(() => { runDiag(); }, []);

  const DISPLAY_NAMES: Record<string, string> = {
    "State DB": "状态数据库",
    "SiYuan Kernel": "知识引擎",
    "Codex": "Codex",
    "OpenCode": "OpenCode",
    "Gemini CLI": "Gemini CLI",
    "Claude Code": "Claude Code",
  };

  return (
    <div className="p-6 max-w-xl">
      <div className="flex items-center justify-between mb-6">
        <div>
          <h1 className="text-xl font-semibold">帮助与诊断</h1>
          <p className="text-xs text-gray-400 mt-0.5">系统组件状态</p>
        </div>
        <button
          onClick={runDiag}
          disabled={loading}
          className="flex items-center gap-1.5 px-3 py-1.5 border border-gray-300 dark:border-gray-600 rounded-lg text-sm hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors"
        >
          <RefreshCw className={`w-3.5 h-3.5 ${loading ? "animate-spin" : ""}`} />
          重新检测
        </button>
      </div>

      <div className="space-y-2 mb-4">
        {doctor?.checks.map((check) => (
          <div key={check.name} className="flex items-center gap-3 bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 px-4 py-3">
            {check.ok ? (
              <CheckCircle className="w-4 h-4 text-green-500 flex-shrink-0" />
            ) : (
              <XCircle className="w-4 h-4 text-yellow-500 flex-shrink-0" />
            )}
            <div className="flex-1 min-w-0">
              <div className="text-sm font-medium">{DISPLAY_NAMES[check.name] ?? check.name}</div>
              {!check.ok && check.message !== "OK" && (
                <div className="text-xs text-gray-400 truncate">{check.message}</div>
              )}
            </div>
            <div className={`text-xs ${check.ok ? "text-green-600" : "text-yellow-500"}`}>
              {check.ok ? "正常" : "注意"}
            </div>
          </div>
        ))}

        {/* AI status */}
        <div className="flex items-center gap-3 bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 px-4 py-3">
          {aiOk === null ? (
            <Cpu className="w-4 h-4 text-gray-400 flex-shrink-0" />
          ) : aiOk ? (
            <CheckCircle className="w-4 h-4 text-green-500 flex-shrink-0" />
          ) : (
            <XCircle className="w-4 h-4 text-yellow-500 flex-shrink-0" />
          )}
          <div className="flex-1">
            <div className="text-sm font-medium">AI 智能整理</div>
            {aiOk === false && (
              <div className="text-xs text-gray-400">无法连接公司内部 AI 服务</div>
            )}
          </div>
          <div className={`text-xs ${aiOk ? "text-green-600" : aiOk === false ? "text-yellow-500" : "text-gray-400"}`}>
            {aiOk === null ? "检测中" : aiOk ? "正常" : "暂时不可用"}
          </div>
        </div>
      </div>

      <div className="space-y-2">
        <button
          onClick={() => invoke("open_data_folder")}
          className="w-full flex items-center gap-2 px-4 py-2.5 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-lg text-sm hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors"
        >
          <FileText className="w-4 h-4 text-gray-400" />
          打开数据目录
        </button>
        <button
          onClick={() => invoke("restart_siyuan")}
          className="w-full flex items-center gap-2 px-4 py-2.5 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-lg text-sm hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors"
        >
          <RefreshCw className="w-4 h-4 text-gray-400" />
          重新启动知识引擎
        </button>
      </div>

      <div className="mt-4 p-3 bg-blue-50 dark:bg-blue-900/20 rounded-lg text-xs text-blue-600 dark:text-blue-400">
        💡 原始会话仍会正常保存，AI 整理不可用不影响主要功能。
      </div>
    </div>
  );
}
