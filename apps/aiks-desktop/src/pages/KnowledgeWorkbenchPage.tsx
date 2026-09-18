import { useCallback, useEffect, useMemo, useState } from "react";
import { Archive, Bot, FilePenLine, Plus, RefreshCw, Star } from "lucide-react";
import { getApi } from "../api/client";
import type { KnowledgePage, KnowledgeSummary } from "../api/types";
import KnowledgeEditor from "../components/KnowledgeEditor";

const CATEGORY_LABELS: Record<string, string> = {
  troubleshooting: "故障排查",
  architecture: "架构设计",
  implementation: "实现方案",
  configuration: "配置管理",
  research: "技术探索",
  decision: "决策记录",
  general: "通用",
};

function parseTags(tags: string): string[] {
  try { return JSON.parse(tags); } catch { return tags ? [tags] : []; }
}

type ViewFilter = "all" | "ai" | "manual" | "favorite" | "archived";

interface Props {
  onViewDetail?: (id: string) => void;
}

function SourceBadge({ item }: { item: KnowledgeSummary }) {
  if (item.source_type === "manual") {
    return <span className="inline-flex items-center gap-1 rounded bg-emerald-50 px-1.5 py-0.5 text-[10px] font-medium text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-300"><FilePenLine className="h-3 w-3" />手动创建</span>;
  }
  return <span className="inline-flex items-center gap-1 rounded bg-blue-50 px-1.5 py-0.5 text-[10px] font-medium text-blue-700 dark:bg-blue-900/30 dark:text-blue-300"><Bot className="h-3 w-3" />AI 提炼</span>;
}

export default function KnowledgeWorkbenchPage({ onViewDetail }: Props) {
  const [view, setView] = useState<ViewFilter>("all");
  const [category, setCategory] = useState("");
  const [data, setData] = useState<KnowledgePage | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const [page, setPage] = useState(0);
  const PAGE_SIZE = 30;

  const filters = useMemo(() => {
    const base = {
      category: category || undefined,
      limit: PAGE_SIZE,
      offset: page * PAGE_SIZE,
    };
    switch (view) {
      case "ai": return { ...base, status: "active" as const, sourceType: "conversation" as const };
      case "manual": return { ...base, status: "active" as const, sourceType: "manual" as const };
      case "favorite": return { ...base, status: "active" as const, favorite: true };
      case "archived": return { ...base, status: "archived" as const };
      default: return { ...base, status: "active" as const };
    }
  }, [view, category, page]);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      setData(await getApi().getKnowledge(filters));
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [filters]);

  useEffect(() => { void load(); }, [load]);

  const toggleFavorite = async (item: KnowledgeSummary) => {
    try {
      await getApi().setKnowledgeFavorite(item.id, !item.is_favorite);
      await load();
    } catch (e) {
      setError(String(e));
    }
  };

  const tabs: Array<[ViewFilter, string]> = [
    ["all", "全部"], ["ai", "AI 提炼"], ["manual", "手动创建"], ["favorite", "收藏"], ["archived", "已归档"],
  ];

  return (
    <div className="flex h-full min-h-0 flex-col p-6">
      <div className="mb-4 flex items-start justify-between gap-4">
        <div>
          <div className="flex items-center gap-2">
            <h1 className="text-lg font-semibold text-gray-900 dark:text-gray-100">知识库</h1>
            <span className="rounded bg-violet-50 px-1.5 py-0.5 text-[10px] font-semibold text-violet-700 dark:bg-violet-900/30 dark:text-violet-300">V4 Native</span>
          </div>
          <p className="mt-1 text-sm text-gray-500">AI 提炼与手工知识统一管理；SiYuan 作为可选发布目标，不再是第二套编辑器。</p>
        </div>
        <button
          type="button"
          onClick={() => setCreating(true)}
          className="inline-flex items-center gap-1.5 rounded-lg bg-blue-600 px-4 py-2 text-sm font-medium text-white shadow-sm hover:bg-blue-700"
        >
          <Plus className="h-4 w-4" />
          新建知识
        </button>
      </div>

      <div className="mb-4 flex flex-wrap items-center justify-between gap-3 border-b border-gray-200 pb-3 dark:border-gray-700">
        <div className="flex flex-wrap gap-1">
          {tabs.map(([key, label]) => (
            <button
              key={key}
              type="button"
              onClick={() => { setView(key); setPage(0); }}
              className={`rounded-md px-3 py-1.5 text-xs font-medium transition-colors ${view === key
                ? "bg-gray-900 text-white dark:bg-gray-100 dark:text-gray-900"
                : "text-gray-500 hover:bg-gray-100 hover:text-gray-800 dark:hover:bg-gray-800 dark:hover:text-gray-200"}`}
            >
              {label}
            </button>
          ))}
        </div>
        <div className="flex items-center gap-2">
          <select
            value={category}
            onChange={e => { setCategory(e.target.value); setPage(0); }}
            className="rounded-md border border-gray-200 bg-white px-2.5 py-1.5 text-xs text-gray-600 dark:border-gray-700 dark:bg-gray-800 dark:text-gray-300"
          >
            <option value="">全部分类</option>
            {Object.entries(CATEGORY_LABELS).map(([key, label]) => <option key={key} value={key}>{label}</option>)}
          </select>
          <button type="button" onClick={() => void load()} className="rounded-md border border-gray-200 p-1.5 text-gray-400 hover:bg-gray-50 dark:border-gray-700 dark:hover:bg-gray-800" title="刷新">
            <RefreshCw className={`h-3.5 w-3.5 ${loading ? "animate-spin" : ""}`} />
          </button>
        </div>
      </div>

      {error && <div className="mb-4 rounded-md border border-red-200 bg-red-50 px-4 py-2 text-xs text-red-700 dark:border-red-900/50 dark:bg-red-900/20 dark:text-red-300">{error}</div>}

      <div className="mb-3 flex items-center justify-between text-xs text-gray-400">
        <span>{data ? `共 ${data.total} 条知识` : "正在读取本地知识库..."}</span>
        <span>发布到 SiYuan 请进入知识详情</span>
      </div>

      {loading && !data ? (
        <div className="flex flex-1 items-center justify-center text-sm text-gray-400">加载中...</div>
      ) : data?.items.length === 0 ? (
        <div className="flex flex-1 flex-col items-center justify-center rounded-xl border border-dashed border-gray-300 text-center dark:border-gray-700">
          <div className="rounded-full bg-gray-100 p-3 dark:bg-gray-800"><FilePenLine className="h-6 w-6 text-gray-400" /></div>
          <p className="mt-3 text-sm font-medium text-gray-600 dark:text-gray-300">当前视图暂无知识</p>
          <p className="mt-1 text-xs text-gray-400">可以从工作记录自动提炼，也可以直接在 AIKS 中新建。</p>
          <button type="button" onClick={() => setCreating(true)} className="mt-4 text-sm font-medium text-blue-600 hover:text-blue-700">+ 新建第一条知识</button>
        </div>
      ) : (
        <div className="grid grid-cols-1 gap-3 overflow-y-auto pb-2 lg:grid-cols-2 xl:grid-cols-3">
          {data?.items.map(item => {
            const tags = parseTags(item.tags);
            return (
              <article
                key={item.id}
                onClick={() => onViewDetail?.(item.id)}
                className="group cursor-pointer rounded-lg border border-gray-200 bg-white p-4 transition hover:-translate-y-0.5 hover:border-blue-300 hover:shadow-sm dark:border-gray-700 dark:bg-gray-800 dark:hover:border-blue-600"
              >
                <div className="flex items-start justify-between gap-3">
                  <div className="min-w-0 flex-1">
                    <div className="mb-2 flex flex-wrap items-center gap-1.5">
                      <SourceBadge item={item} />
                      <span className="rounded bg-gray-100 px-1.5 py-0.5 text-[10px] text-gray-500 dark:bg-gray-700 dark:text-gray-300">{CATEGORY_LABELS[item.category] ?? item.category}</span>
                      {item.managed_by === "user" && item.source_type === "conversation" && <span className="rounded bg-amber-50 px-1.5 py-0.5 text-[10px] text-amber-700 dark:bg-amber-900/30 dark:text-amber-300">已人工编辑</span>}
                      {item.status === "archived" && <span className="inline-flex items-center gap-0.5 rounded bg-gray-100 px-1.5 py-0.5 text-[10px] text-gray-500"><Archive className="h-2.5 w-2.5" />已归档</span>}
                    </div>
                    <h3 className="line-clamp-2 text-sm font-semibold leading-5 text-gray-900 dark:text-gray-100">{item.title}</h3>
                  </div>
                  <button
                    type="button"
                    onClick={e => { e.stopPropagation(); void toggleFavorite(item); }}
                    className={`rounded p-1 transition ${item.is_favorite ? "text-amber-500" : "text-gray-300 opacity-50 group-hover:opacity-100 hover:text-amber-500"}`}
                    title={item.is_favorite ? "取消收藏" : "收藏"}
                  >
                    <Star className="h-4 w-4" fill={item.is_favorite ? "currentColor" : "none"} />
                  </button>
                </div>

                <p className="mt-2 line-clamp-3 text-xs leading-5 text-gray-500 dark:text-gray-400">{item.summary || "暂无摘要"}</p>

                <div className="mt-3 flex flex-wrap gap-1">
                  {tags.slice(0, 4).map(tag => <span key={tag} className="rounded bg-gray-50 px-1.5 py-0.5 text-[10px] text-gray-400 dark:bg-gray-900/40">#{tag}</span>)}
                </div>

                <div className="mt-4 flex items-center justify-between border-t border-gray-100 pt-3 text-[10px] text-gray-400 dark:border-gray-700">
                  <span>{item.project_name || "未归属项目"}</span>
                  <span>{new Date(item.updated_at).toLocaleDateString("zh-CN")}</span>
                </div>
              </article>
            );
          })}
        </div>
      )}

      {data && data.total > PAGE_SIZE && (
        <div className="mt-4 flex items-center justify-end gap-2">
          <button onClick={() => setPage(value => Math.max(0, value - 1))} disabled={page === 0} className="rounded border border-gray-200 px-3 py-1.5 text-xs disabled:opacity-40 dark:border-gray-700">上一页</button>
          <span className="text-xs text-gray-400">{page + 1}</span>
          <button onClick={() => setPage(value => value + 1)} disabled={(page + 1) * PAGE_SIZE >= data.total} className="rounded border border-gray-200 px-3 py-1.5 text-xs disabled:opacity-40 dark:border-gray-700">下一页</button>
        </div>
      )}

      <KnowledgeEditor
        open={creating}
        onClose={() => setCreating(false)}
        onSaved={item => {
          setCreating(false);
          void load();
          onViewDetail?.(item.id);
        }}
      />
    </div>
  );
}
