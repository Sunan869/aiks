import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { BookOpen, ExternalLink, Loader2 } from "lucide-react";

interface KnowledgeItem {
  source: string; session_id: string; category: string;
  score: number; doc_id: string; updated_at: string;
}

const CATEGORY_LABELS: Record<string, string> = {
  troubleshooting: "故障排查", implementation: "实现",
  design: "设计", research: "研究", decision: "决策", general: "通用",
};

export default function KnowledgePage() {
  const [items, setItems] = useState<KnowledgeItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [siyuanUrl, setSiyuanUrl] = useState<string | null>(null);
  const [extracting, setExtracting] = useState<string | null>(null);
  const [stats, setStats] = useState<any>(null);

  useEffect(() => {
    Promise.all([
      invoke<KnowledgeItem[]>("get_recent_knowledge", { limit: 50 }),
      invoke<string | null>("get_siyuan_url"),
      invoke("get_knowledge_stats"),
    ]).then(([k, url, s]) => {
      setItems(k);
      setSiyuanUrl(url);
      setStats(s);
    }).catch(console.error)
      .finally(() => setLoading(false));
  }, []);

  const openKnowledge = async () => {
    await invoke("open_knowledge_window");
  };

  if (loading) {
    return (
      <div className="flex items-center justify-center h-full">
        <Loader2 className="w-6 h-6 animate-spin text-gray-400" />
      </div>
    );
  }

  return (
    <div className="p-6 max-w-2xl">
      <div className="flex items-center justify-between mb-6">
        <div>
          <h1 className="text-xl font-semibold flex items-center gap-2">
            <BookOpen className="w-5 h-5" />
            知识库
          </h1>
          <p className="text-xs text-gray-400 mt-0.5">AI 精炼的工程知识</p>
        </div>
        <button
          onClick={openKnowledge}
          className="flex items-center gap-1.5 px-3 py-1.5 bg-blue-600 hover:bg-blue-700 text-white rounded-lg text-xs transition-colors"
        >
          <ExternalLink className="w-3 h-3" />
          打开知识库
        </button>
      </div>

      {/* Stats */}
      {stats && (
        <div className="grid grid-cols-4 gap-2 mb-4">
          {[
            { label: "精炼知识", value: stats.success, color: "text-green-600" },
            { label: "已跳过", value: stats.skipped, color: "text-gray-400" },
            { label: "待整理", value: stats.pending, color: "text-blue-600" },
            { label: "失败", value: stats.failed, color: "text-red-500" },
          ].map((item) => (
            <div key={item.label} className="bg-white dark:bg-gray-800 rounded border border-gray-200 dark:border-gray-700 p-3 text-center">
              <div className={`text-xl font-bold ${item.color}`}>{item.value}</div>
              <div className="text-xs text-gray-400 mt-0.5">{item.label}</div>
            </div>
          ))}
        </div>
      )}

      {items.length === 0 ? (
        <div className="text-center py-12 text-gray-400">
          <BookOpen className="w-12 h-12 mx-auto mb-3 opacity-30" />
          <div className="text-sm font-medium mb-1">暂无精炼知识</div>
          <div className="text-xs">同步数据后，AI 将自动整理有价值的会话</div>
          <div className="text-xs mt-1">或点击数据源中的「立即整理」</div>
        </div>
      ) : (
        <div className="space-y-2">
          {items.map((item, i) => (
            <div
              key={i}
              className="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 p-3 hover:border-blue-300 transition-colors cursor-pointer"
              onClick={openKnowledge}
            >
              <div className="flex items-start justify-between gap-2">
                <div className="flex-1 min-w-0">
                  <div className="text-sm font-medium text-gray-800 dark:text-gray-200 truncate">
                    {item.session_id}
                  </div>
                  <div className="flex items-center gap-2 mt-1 text-xs text-gray-400">
                    <span className="px-1.5 py-0.5 bg-gray-100 dark:bg-gray-700 rounded text-xs">
                      {CATEGORY_LABELS[item.category] ?? item.category}
                    </span>
                    <span>{item.source}</span>
                    <span>{new Date(item.updated_at).toLocaleDateString("zh-CN")}</span>
                  </div>
                </div>
                <div className="text-xs text-gray-400 flex-shrink-0">
                  {item.score?.toFixed(2)}
                </div>
              </div>
            </div>
          ))}
          <div className="text-center pt-2">
            <button
              onClick={openKnowledge}
              className="text-xs text-blue-500 hover:text-blue-600"
            >
              在知识库中查看全部 →
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
