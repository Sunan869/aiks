import { useState, useCallback } from "react";
import { getApi } from "../api/client";
import type { SearchResponse, SearchResult } from "../api/types";
import SiYuanWorkspace from "../components/SiYuanWorkspace";

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

interface Props { onViewKnowledge?: (id: string) => void; }
type SearchSurface = "aiks" | "siyuan";

export default function SearchPage({ onViewKnowledge }: Props) {
  const [surface, setSurface] = useState<SearchSurface>("aiks");
  const [query, setQuery] = useState("");
  const [result, setResult] = useState<SearchResponse | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const search = useCallback(async () => {
    if (!query.trim()) return;
    setLoading(true);
    setError(null);
    try {
      const r = await getApi().searchKnowledge(query.trim(), 20);
      setResult(r);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [query]);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter") search();
  };

  return (
    <div className="flex h-full min-h-0 flex-col p-6">
      <div className="mb-4">
        <h1 className="text-lg font-semibold text-gray-900 dark:text-gray-100">搜索</h1>
        <p className="mt-0.5 text-sm text-gray-500">
          {surface === "aiks"
            ? "搜索 AIKS 提炼知识并保留来源追踪"
            : "直接使用 SiYuan 原生全库搜索，不复制其索引与搜索语义"}
        </p>
      </div>

      <div className="mb-5 flex items-center gap-1 self-start rounded-lg bg-gray-100 p-1 dark:bg-gray-800">
        <button
          type="button"
          onClick={() => setSurface("aiks")}
          className={`rounded-md px-3 py-1.5 text-xs font-medium transition-colors ${surface === "aiks"
            ? "bg-white text-gray-900 shadow-sm dark:bg-gray-700 dark:text-gray-100"
            : "text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200"}`}
        >
          AIKS 搜索
        </button>
        <button
          type="button"
          onClick={() => setSurface("siyuan")}
          className={`rounded-md px-3 py-1.5 text-xs font-medium transition-colors ${surface === "siyuan"
            ? "bg-white text-gray-900 shadow-sm dark:bg-gray-700 dark:text-gray-100"
            : "text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200"}`}
        >
          SiYuan 全库
        </button>
      </div>

      {surface === "siyuan" ? (
        <SiYuanWorkspace
          title="SiYuan 全库搜索"
          description="在原生 SiYuan 工作区中搜索手工笔记、AIKS 同步知识和其他已有内容。"
        />
      ) : (
        <div className="w-full max-w-3xl self-center">
          <div className="flex gap-2 mb-6">
            <input
              type="text"
              value={query}
              onChange={e => setQuery(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder="搜索 AIKS 提炼知识... (支持关键词)"
              className="flex-1 px-4 py-2.5 border border-gray-300 dark:border-gray-600 rounded-lg text-sm bg-white dark:bg-gray-800 text-gray-900 dark:text-gray-100 placeholder-gray-400 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent"
              autoFocus
            />
            <button
              onClick={search}
              disabled={!query.trim() || loading}
              className="px-5 py-2.5 bg-blue-600 hover:bg-blue-700 disabled:bg-gray-300 dark:disabled:bg-gray-600 text-white text-sm rounded-lg transition-colors font-medium"
            >
              {loading ? "搜索中..." : "搜索"}
            </button>
          </div>

          {error && (
            <div className="mb-4 px-4 py-3 bg-red-50 dark:bg-red-900/20 border border-red-200 rounded text-sm text-red-700 dark:text-red-400">
              {error}
            </div>
          )}

          {result && (
            <div>
              <div className="text-xs text-gray-500 mb-3">
                找到 {result.total} 条结果，搜索词："{result.query}"
              </div>

              {result.results.length === 0 ? (
                <div className="text-center py-12 text-gray-400">
                  <p className="text-sm">没有找到相关提炼知识</p>
                  <p className="text-xs mt-1">可以尝试不同关键词，或切换到 SiYuan 全库搜索手工笔记</p>
                </div>
              ) : (
                <div className="space-y-3">
                  {result.results.map((item: SearchResult) => {
                    const tags = parseTags(item.tags);
                    return (
                      <div key={item.id}
                        className="p-4 bg-white dark:bg-gray-800 rounded border border-gray-200 dark:border-gray-700 hover:border-blue-300 dark:hover:border-blue-600 transition-colors cursor-pointer"
                        onClick={() => onViewKnowledge?.(item.id)}
                      >
                        <div className="flex items-start gap-2">
                          <div className="flex-1 min-w-0">
                            <h3 className="text-sm font-medium text-gray-900 dark:text-gray-100">{item.title}</h3>
                            <p className="text-xs text-gray-500 dark:text-gray-400 mt-1.5 line-clamp-2 leading-relaxed">
                              {item.summary}
                            </p>
                            <div className="flex items-center gap-2 mt-2">
                              {item.project_name && (
                                <span className="text-[10px] text-gray-400">{item.project_name}</span>
                              )}
                              {tags.slice(0, 3).map((tag: string) => (
                                <span key={tag} className="text-[10px] px-1.5 py-0.5 bg-gray-100 dark:bg-gray-700 text-gray-500 dark:text-gray-400 rounded">
                                  {tag}
                                </span>
                              ))}
                            </div>
                          </div>
                          <div className="flex-shrink-0 text-right">
                            <span className="text-[10px] px-1.5 py-0.5 bg-gray-100 dark:bg-gray-700 text-gray-500 dark:text-gray-400 rounded">
                              {CATEGORY_LABELS[item.category] ?? item.category}
                            </span>
                            <div className="text-[10px] text-gray-400 mt-1">
                              {(item.confidence * 100).toFixed(0)}% 置信度
                            </div>
                          </div>
                        </div>
                      </div>
                    );
                  })}
                </div>
              )}
            </div>
          )}

          {!result && !loading && (
            <div className="text-center py-12 text-gray-400">
              <p className="text-sm">输入关键词搜索 AIKS 提炼知识</p>
              <p className="text-xs mt-1">支持标题、摘要、内容和标签；完整笔记库请切换到 SiYuan 全库</p>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
