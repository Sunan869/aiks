import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { RefreshCw } from "lucide-react";
import type { FullStatus } from "../App";

interface Props {
  fullStatus: FullStatus | null;
  onRefresh: () => void;
}

export default function SyncPage({ fullStatus, onRefresh }: Props) {
  const [syncing, setSyncing] = useState(false);
  const [result, setResult] = useState<any>(null);

  const doSync = async () => {
    setSyncing(true);
    setResult(null);
    try {
      const r = await invoke<any>("sync_and_extract");
      setResult(r);
      setTimeout(onRefresh, 500);
    } catch (e) {
      setResult({ error: String(e) });
    } finally { setSyncing(false); }
  };

  return (
    <div className="p-6 max-w-2xl">
      <div className="mb-6">
        <h1 className="text-xl font-semibold">同步记录</h1>
        <p className="text-xs text-gray-400 mt-0.5">Raw Session 同步与 AI 整理状态</p>
      </div>

      {/* Sync stats */}
      {fullStatus && (
        <>
          <div className="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 p-4 mb-4">
            <div className="font-medium text-sm mb-3">最近同步</div>
            {fullStatus.last_sync_at ? (
              <>
                <div className="text-xs text-gray-500 mb-2">
                  {new Date(fullStatus.last_sync_at).toLocaleString("zh-CN")}
                </div>
                <div className="grid grid-cols-4 gap-2 text-center text-xs">
                  {[
                    { label: "发现", value: fullStatus.last_sync_discovered },
                    { label: "新增", value: fullStatus.last_sync_new },
                    { label: "更新", value: fullStatus.last_sync_updated },
                    { label: "失败", value: fullStatus.last_sync_failed },
                  ].map(item => (
                    <div key={item.label} className="bg-gray-50 dark:bg-gray-700 rounded p-2">
                      <div className="text-xl font-bold text-blue-600">{item.value}</div>
                      <div className="text-gray-400">{item.label}</div>
                    </div>
                  ))}
                </div>
              </>
            ) : (
              <div className="text-sm text-gray-400">尚未执行同步</div>
            )}
          </div>

          {/* AI extraction stats */}
          <div className="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 p-4 mb-4">
            <div className="font-medium text-sm mb-3">AI 智能整理</div>
            <div className="grid grid-cols-4 gap-2 text-center text-xs">
              {[
                { label: "待整理", value: fullStatus.extraction_pending, color: "text-blue-600" },
                { label: "成功", value: fullStatus.extraction_success, color: "text-green-600" },
                { label: "已跳过", value: fullStatus.extraction_skipped, color: "text-gray-400" },
                { label: "失败", value: fullStatus.extraction_failed, color: "text-red-500" },
              ].map(item => (
                <div key={item.label} className="bg-gray-50 dark:bg-gray-700 rounded p-2">
                  <div className={`text-xl font-bold ${item.color}`}>{item.value}</div>
                  <div className="text-gray-400">{item.label}</div>
                </div>
              ))}
            </div>
          </div>
        </>
      )}

      {result && !result.error && (
        <div className="mb-4 bg-green-50 dark:bg-green-900/20 rounded-lg p-3 text-xs text-green-700 dark:text-green-400">
          本次同步：发现 {result.discovered}，新增 {result.new_count}，更新 {result.updated_count}，
          跳过 {result.skipped_count}，失败 {result.failed_count}
          {result.extraction_queued > 0 && `，${result.extraction_queued} 条排队 AI 整理`}
        </div>
      )}
      {result?.error && (
        <div className="mb-4 text-xs text-red-500">{result.error}</div>
      )}

      <button onClick={doSync} disabled={syncing}
        className="flex items-center gap-2 px-4 py-2 bg-blue-600 hover:bg-blue-700 disabled:opacity-50 text-white rounded-lg text-sm transition-colors"
      >
        <RefreshCw className={`w-4 h-4 ${syncing ? "animate-spin" : ""}`} />
        {syncing ? "同步中..." : "立即同步"}
      </button>

      <div className="mt-3 text-xs text-gray-400">
        同步完成后，AIKS 会自动对有价值的会话排队进行 AI 智能整理。
      </div>
    </div>
  );
}
