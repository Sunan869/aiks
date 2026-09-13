import { useState } from "react";
import { CheckCircle, XCircle, RefreshCw } from "lucide-react";
import { getApi } from "../api/client";
import type { FullStatus } from "../api/types";

interface Props {
  fullStatus: FullStatus | null;
}

const SOURCE_ICONS: Record<string, string> = {
  "Codex": "📦", "OpenCode": "🔮", "Gemini CLI": "✨", "Claude Code": "🤖",
};
const SOURCE_DESCS: Record<string, string> = {
  "Codex": "OpenAI Codex CLI (~/.codex)",
  "OpenCode": "OpenCode 数据库",
  "Gemini CLI": "Google Gemini CLI (~/.gemini)",
  "Claude Code": "Claude Code (~/.claude)",
};

export default function SourcesPage({ fullStatus }: Props) {
  const [syncing, setSyncing] = useState<Record<string, boolean>>({});
  const [msgs, setMsgs] = useState<Record<string, string>>({});

  const handleSync = async (source: string) => {
    const srcKey = source === "Gemini CLI" ? "gemini_cli" :
                   source === "Claude Code" ? "claude_code" :
                   source.toLowerCase();
    setSyncing(s => ({ ...s, [source]: true }));
    setMsgs(m => ({ ...m, [source]: "" }));
    try {
      const r = await getApi().syncAndExtract(srcKey);
      setMsgs(m => ({ ...m, [source]: `扫描完成：+${r.new_count} 新增，${r.updated_count} 更新` }));
    } catch (e) {
      setMsgs(m => ({ ...m, [source]: `错误：${e}` }));
    } finally { setSyncing(s => ({ ...s, [source]: false })); }
  };

  const sources = ["OpenCode", "Codex", "Gemini CLI", "Claude Code"];

  return (
    <div className="p-6 max-w-2xl">
      <div className="mb-6">
        <h1 className="text-xl font-semibold">数据源</h1>
        <p className="text-xs text-gray-400 mt-0.5">AI 工具会话目录状态</p>
      </div>

      <div className="space-y-3">
        {sources.map((src) => {
          const count = fullStatus?.scan_by_source[src] ?? 0;
          const detected = count > 0;
          const isSyncing = syncing[src];
          const msg = msgs[src];

          return (
            <div key={src} className="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 p-4">
              <div className="flex items-center justify-between mb-1">
                <div className="flex items-center gap-2 font-medium text-sm">
                  <span>{SOURCE_ICONS[src]}</span> {src}
                </div>
                <div className={`flex items-center gap-1 text-xs ${detected ? "text-green-600" : "text-gray-400"}`}>
                  {detected ? <CheckCircle className="w-3 h-3" /> : <XCircle className="w-3 h-3" />}
                  {detected ? "已连接" : "未检测到"}
                </div>
              </div>
              <div className="text-xs text-gray-400 mb-2">{SOURCE_DESCS[src]}</div>

              {detected ? (
                <div className="flex items-center justify-between">
                  <span className="text-sm">
                    <span className="font-medium text-blue-600">{count}</span> 条历史对话
                    {fullStatus && fullStatus.db_synced > 0 && (
                      <span className="text-gray-400 ml-2">· {fullStatus.db_synced} 已同步</span>
                    )}
                  </span>
                  <button onClick={() => handleSync(src)} disabled={isSyncing}
                    className="flex items-center gap-1 px-2.5 py-1 text-xs border border-blue-300 dark:border-blue-600 text-blue-600 dark:text-blue-400 rounded hover:bg-blue-50 dark:hover:bg-blue-900/30 disabled:opacity-50"
                  >
                    <RefreshCw className={`w-3 h-3 ${isSyncing ? "animate-spin" : ""}`} />
                    {isSyncing ? "同步中..." : "立即同步"}
                  </button>
                </div>
              ) : (
                <div className="text-xs text-gray-400 italic">安装 {src} 后 AIKS 会自动识别</div>
              )}
              {msg && <div className="text-xs text-gray-500 mt-1">{msg}</div>}
            </div>
          );
        })}
      </div>
    </div>
  );
}
