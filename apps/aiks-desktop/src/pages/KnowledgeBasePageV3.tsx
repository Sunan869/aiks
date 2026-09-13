import { useState, useEffect, useCallback } from "react";
import { getApi } from "../api/client";
import type { KnowledgeSummary, KnowledgePage } from "../api/types";

const CATEGORY_LABELS: Record<string, string> = {
  troubleshooting: "故障排查",
  architecture: "架构设计",
  implementation: "实现方案",
  configuration: "配置管理",
  research: "技术探索",
  decision: "决策记录",
  general: "通用",
};

const CATEGORY_COLORS: Record<string, string> = {
  troubleshooting: "bg-red-50 text-red-700 dark:bg-red-900/30 dark:text-red-400",
  architecture: "bg-purple-50 text-purple-700 dark:bg-purple-900/30 dark:text-purple-400",
  implementation: "bg-blue-50 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400",
  configuration: "bg-yellow-50 text-yellow-700 dark:bg-yellow-900/30 dark:text-yellow-400",
  research: "bg-teal-50 text-teal-700 dark:bg-teal-900/30 dark:text-teal-400",
  decision: "bg-indigo-50 text-indigo-700 dark:bg-indigo-900/30 dark:text-indigo-400",
  general: "bg-gray-100 text-gray-600 dark:bg-gray-700 dark:text-gray-400",
};

function parseTags(tags: string): string[] {
  try { return JSON.parse(tags); } catch { return tags ? [tags] : []; }
}

function KnowledgeCard({ item }: { item: KnowledgeSummary }) {
  const tags = parseTags(item.tags);
  return (
    <div className="p-4 bg-white dark:bg-gray-800 rounded border border-gray-200 dark:border-gray-700 hover:border-blue-300 dark:hover:border-blue-600 transition-colors cursor-pointer">
      <div className="flex items-start justify-between gap-2">
        <h3 className="text-sm font-medium text-gray-900 dark:text-gray-100 leading-snug">{item.title}</h3>
        <span className={`flex-shrink-0 text-[10px] px-1.5 py-0.5 rounded ${CATEGORY_COLORS[item.category] ?? CATEGORY_COLORS.general}`}>
          {CATEGORY_LABELS[item.category] ?? item.category}
        </span>
      </div>
      <p className="text-xs text-gray-500 dark:text-gray-400 mt-2 line-clamp-2 leading-relaxed">{item.summary}</p>
      <div className="flex items-center justify-between mt-3">
        <div className="flex gap-1 flex-wrap">
          {tags.slice(0, 3).map((tag: string) => (
            <span key={tag} className="text-[10px] px-1.5 py-0.5 bg-gray-100 dark:bg-gray-700 text-gray-500 dark:text-gray-400 rounded">
              {tag}
            </span>
          ))}
        </div>
        <span className="text-[10px] text-gray-400">
          {item.project_name && <span className="mr-2">{item.project_name}</span>}
          {new Date(item.updated_at).toLocaleDateString("zh-CN")}
        </span>
      </div>
    </div>
  );
}

export default function KnowledgeBasePageV3() {
  const [data, setData] = useState<KnowledgePage | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [page, setPage] = useState(0);
  const [category, setCategory] = useState("");
  const PAGE_SIZE = 30;

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const result = await getApi().getKnowledge({ category: category || undefined, limit: PAGE_SIZE, offset: page * PAGE_SIZE });
      setData(result);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [page, category]);

  useEffect(() => { load(); }, [load]);

  const categories = ["", ...Object.keys(CATEGORY_LABELS)];

  return (
    <div className="p-6">
      <div className="flex items-center justify-between mb-4">
        <div>
          <h1 className="text-lg font-semibold text-gray-900 dark:text-gray-100">知识库</h1>
          {data && <p className="text-sm text-gray-500 mt-0.5">共 {data.total} 条知识</p>}
        </div>
        <select
          value={category}
          onChange={e => { setCategory(e.target.value); setPage(0); }}
          className="text-sm border border-gray-200 dark:border-gray-700 rounded px-2 py-1 bg-white dark:bg-gray-800 text-gray-700 dark:text-gray-300"
        >
          {categories.map(c => (
            <option key={c} value={c}>{c ? CATEGORY_LABELS[c] ?? c : "全部分类"}</option>
          ))}
        </select>
      </div>

      {error && (
        <div className="mb-4 px-4 py-3 bg-red-50 dark:bg-red-900/20 border border-red-200 rounded text-sm text-red-700">
          {error}
        </div>
      )}

      {loading ? (
        <div className="flex items-center justify-center h-48 text-gray-400 text-sm">加载中...</div>
      ) : data?.total === 0 ? (
        <div className="flex flex-col items-center justify-center h-48 text-gray-400">
          <p className="text-sm">暂无知识条目</p>
          <p className="text-xs mt-1">请先处理工作记录以提炼知识</p>
        </div>
      ) : (
        <div className="grid grid-cols-1 lg:grid-cols-2 xl:grid-cols-3 gap-3">
          {data?.items.map((item: KnowledgeSummary) => (
            <KnowledgeCard key={item.id} item={item} />
          ))}
        </div>
      )}

      {data && data.total > PAGE_SIZE && (
        <div className="flex items-center justify-between mt-6">
          <span className="text-xs text-gray-500">
            第 {page * PAGE_SIZE + 1}–{Math.min((page + 1) * PAGE_SIZE, data.total)} 条
          </span>
          <div className="flex gap-2">
            <button onClick={() => setPage(p => p - 1)} disabled={page === 0}
              className="px-3 py-1 text-xs border border-gray-200 rounded disabled:opacity-40 hover:bg-gray-50">
              上一页
            </button>
            <button onClick={() => setPage(p => p + 1)} disabled={(page + 1) * PAGE_SIZE >= data.total}
              className="px-3 py-1 text-xs border border-gray-200 rounded disabled:opacity-40 hover:bg-gray-50">
              下一页
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
