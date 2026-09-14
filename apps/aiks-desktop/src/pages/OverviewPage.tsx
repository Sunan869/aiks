import { useState } from "react";
import { RefreshCw, CheckCircle, Cpu, Zap } from "lucide-react";
import { getApi } from "../api/client";
import type { FullStatus, AiStatus } from "../api/types";

interface Props {
  fullStatus: FullStatus | null;
  aiStatus: AiStatus | null;
  syncInProgress: boolean;
  onRefresh: () => void;
}

export default function OverviewPage({ fullStatus, aiStatus, syncInProgress, onRefresh }: Props) {
  const [syncMsg, setSyncMsg] = useState<string | null>(null);
  const [syncing, setSyncing] = useState(false);

  const handleSync = async () => {
    setSyncing(true);
    setSyncMsg(null);
    try {
      const r = await getApi().syncAndExtract();
      setSyncMsg(`扫描完成：+${r.new_count} 新增，${r.updated_count} 更新`);
      setTimeout(onRefresh, 500);
    } catch (e) {
      setSyncMsg(`错误：${e}`);
    } finally { setSyncing(false); }
  };

  const stats = [
    { label: "工作记录", value: fullStatus?.scan_total ?? 0, icon: "📝" },
    { label: "知识条目", value: fullStatus?.extraction_success ?? 0, icon: "✨" },
    { label: "数据源", value: fullStatus ? Object.values(fullStatus.scan_by_source).filter(v => v > 0).length : 0, icon: "🔗" },
    { label: "待处理", value: fullStatus?.extraction_pending ?? 0, icon: "⏳" },
  ];

  const isSyncing = syncing || syncInProgress;

  return (
    <div className="p-6 space-y-4">
      <div>
        <h1 className="text-xl font-semibold">概览</h1>
        <p className="text-xs text-gray-400 mt-0.5">AI 工作知识库状态</p>
      </div>

      {/* Stats cards */}
      <div className="grid grid-cols-2 gap-3">
        {stats.map((item) => (
          <div key={item.label} className="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 p-4">
            <div className="flex items-center justify-between mb-1">
              <span className="text-xs text-gray-400">{item.label}</span>
              <span>{item.icon}</span>
            </div>
            <div className="text-2xl font-bold text-gray-800 dark:text-gray-100">{item.value}</div>
          </div>
        ))}
      </div>

      {/* Sync status */}
      {fullStatus && (
        <div className="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 p-4">
          <div className="font-medium text-sm mb-2">同步状态</div>
          <div className="grid grid-cols-3 gap-2 text-xs">
            {[
              { label: "已同步", value: fullStatus.db_synced, color: "text-green-600" },
              { label: "待同步", value: fullStatus.db_pending, color: "text-blue-600" },
              { label: "失败", value: fullStatus.db_failed, color: "text-red-500" },
            ].map(item => (
              <div key={item.label} className="bg-gray-50 dark:bg-gray-700 rounded p-2 text-center">
                <div className={`text-lg font-bold ${item.color}`}>{item.value}</div>
                <div className="text-gray-400">{item.label}</div>
              </div>
            ))}
          </div>
          {fullStatus.last_sync_at && (
            <div className="text-xs text-gray-400 mt-2">
              最近同步：{new Date(fullStatus.last_sync_at).toLocaleString("zh-CN")}
              {fullStatus.last_sync_discovered > 0 && ` · 发现 ${fullStatus.last_sync_discovered} 条`}
            </div>
          )}
        </div>
      )}

      {/* AI status */}
      {aiStatus && (
        <div className="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 p-4">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-2 text-sm font-medium">
              <Cpu className="w-4 h-4" />
              AI 智能整理
            </div>
            <div className={`flex items-center gap-1 text-xs ${aiStatus.healthy ? "text-green-600" : "text-yellow-500"}`}>
              <span className={`w-1.5 h-1.5 rounded-full ${aiStatus.healthy ? "bg-green-500" : "bg-yellow-400"}`} />
              {aiStatus.healthy ? "正常" : "暂时不可用"}
            </div>
          </div>
          <div className="text-xs text-gray-400 mt-1">{aiStatus.display_name}</div>
        </div>
      )}

      {syncMsg && (
        <div className="flex items-center gap-2 text-xs text-gray-500">
          <CheckCircle className="w-3 h-3 text-green-500" />
          {syncMsg}
        </div>
      )}

      <button onClick={handleSync} disabled={isSyncing}
        className="flex items-center gap-2 px-4 py-2 bg-blue-600 hover:bg-blue-700 disabled:opacity-50 text-white rounded-lg text-sm transition-colors"
      >
        <RefreshCw className={`w-4 h-4 ${isSyncing ? "animate-spin" : ""}`} />
        {isSyncing ? "同步中..." : "全部同步"}
      </button>
    </div>
  );
}
