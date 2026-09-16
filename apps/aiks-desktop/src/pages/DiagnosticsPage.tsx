import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { CheckCircle, XCircle, RefreshCw, Cpu, FileText } from "lucide-react";
import { getApi } from "../api/client";
import type { V41Diagnostics } from "../api/types";

interface DoctorCheck { name: string; ok: boolean; message: string }
interface DoctorResult { checks: DoctorCheck[]; all_ok: boolean }

export default function DiagnosticsPage() {
  const api = getApi();
  const [doctor, setDoctor] = useState<DoctorResult | null>(null);
  const [aiOk, setAiOk] = useState<boolean | null>(null);
  const [v41, setV41] = useState<V41Diagnostics | null>(null);
  const [loading, setLoading] = useState(false);

  const runDiag = async () => {
    setLoading(true);
    try {
      const [d, ai, workbench] = await Promise.all([
        invoke<DoctorResult>("get_doctor"),
        invoke<boolean>("test_ai_connection"),
        api.getV41Diagnostics(),
      ]);
      setDoctor(d);
      setAiOk(ai);
      setV41(workbench);
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

  const workbenchStatus = !v41?.workbench.available
    ? "不可用"
    : v41.workbench.ready
      ? "Ready"
      : "等待 Bridge";
  const workbenchHealthy = Boolean(v41?.workbench.available && v41.workbench.ready);
  const workspaceMode = v41?.workbench.mode === "session" ? "原始会话" : "知识";
  const upstreamCommit = v41?.siyuan_upstream_commit ?? "";
  const shortCommit = upstreamCommit ? upstreamCommit.slice(0, 12) : "-";

  return (
    <div className="p-6 max-w-2xl">
      <div className="flex items-center justify-between mb-6">
        <div>
          <h1 className="text-xl font-semibold">帮助与诊断</h1>
          <p className="text-xs text-gray-400 mt-0.5">系统组件、知识工作台与迁移状态</p>
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

      <div className="space-y-2 mb-6">
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

      <section className="mb-6">
        <div className="flex items-end justify-between mb-2">
          <div>
            <h2 className="text-sm font-semibold">知识工作台运行时</h2>
            <p className="text-xs text-gray-400 mt-0.5">AIKS Workbench / SiYuan Runtime / Bridge</p>
          </div>
          {v41?.workbench.origin && (
            <span className="max-w-[260px] truncate font-mono text-[10px] text-gray-400" title={v41.workbench.origin}>
              {v41.workbench.origin}
            </span>
          )}
        </div>

        <div className="grid grid-cols-2 gap-2 mb-3">
          <div className="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 px-3 py-2.5">
            <div className="text-[11px] text-gray-400">SiYuan Kernel</div>
            <div className={`text-sm font-medium mt-1 ${v41?.siyuan_ready ? "text-green-600" : "text-yellow-500"}`}>
              {v41 ? (v41.siyuan_ready ? "正常" : "未就绪") : "检测中"}
            </div>
            {v41 && <div className="mt-1 text-[10px] text-gray-400">Base v{v41.siyuan_base_version}</div>}
          </div>
          <div className="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 px-3 py-2.5">
            <div className="text-[11px] text-gray-400">AIKS Workbench</div>
            <div className={`text-sm font-medium mt-1 ${workbenchHealthy ? "text-green-600" : v41 ? "text-yellow-500" : "text-gray-400"}`}>
              {v41 ? workbenchStatus : "检测中"}
            </div>
            {v41 && <div className="mt-1 text-[10px] text-gray-400">v{v41.workbench_version}</div>}
          </div>
          <div className="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 px-3 py-2.5">
            <div className="text-[11px] text-gray-400">Active Bridge</div>
            <div className={`text-sm font-medium mt-1 ${workbenchHealthy ? "text-green-600" : "text-gray-500"}`}>
              {v41 ? `Protocol v${v41.workbench.protocol_version}` : "检测中"}
            </div>
            {v41 && <div className="mt-1 text-[10px] text-gray-400">V4.2 contract v{v41.bridge_protocol_version}</div>}
          </div>
          <div className="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 px-3 py-2.5">
            <div className="text-[11px] text-gray-400">当前工作区</div>
            <div className="text-sm font-medium mt-1">{v41 ? workspaceMode : "检测中"}</div>
            {v41 && (
              <div className="mt-1 font-mono text-[10px] text-gray-400" title={upstreamCommit}>
                upstream {shortCommit}
              </div>
            )}
          </div>
        </div>

        <div className="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 p-3">
          <div className="text-xs font-medium mb-2">知识内容迁移</div>
          <div className="grid grid-cols-3 sm:grid-cols-6 gap-2">
            {[
              ["总计", v41?.migration.total ?? "-", "text-gray-700 dark:text-gray-200"],
              ["待迁移", v41?.migration.pending ?? "-", (v41?.migration.pending ?? 0) > 0 ? "text-yellow-600" : "text-gray-700 dark:text-gray-200"],
              ["已迁移", v41?.migration.migrated ?? "-", "text-green-600"],
              ["复用", v41?.migration.reused ?? "-", "text-blue-600"],
              ["冲突", v41?.migration.conflicts ?? "-", (v41?.migration.conflicts ?? 0) > 0 ? "text-orange-600 font-semibold" : "text-gray-700 dark:text-gray-200"],
              ["失败", v41?.migration.failed ?? "-", (v41?.migration.failed ?? 0) > 0 ? "text-red-600 font-semibold" : "text-gray-700 dark:text-gray-200"],
            ].map(([label, value, valueClass]) => (
              <div key={label as string} className="rounded-md bg-gray-50 dark:bg-gray-900/40 px-2 py-2 text-center">
                <div className={`text-base ${valueClass}`}>{value}</div>
                <div className="text-[10px] text-gray-400 mt-0.5">{label}</div>
              </div>
            ))}
          </div>
        </div>
      </section>

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