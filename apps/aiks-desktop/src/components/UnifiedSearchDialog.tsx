import { useEffect, useMemo, useRef, useState } from "react";
import { FileText, Loader2, MessageSquareText, Search, X } from "lucide-react";
import { searchAllProgressively } from "../api/search-progress";
import type { UnifiedSearchHit, UnifiedSearchOutcome } from "../api/types";

interface Props {
  open: boolean;
  onClose: () => void;
  onSelect: (hit: UnifiedSearchHit) => void;
}

const EMPTY: UnifiedSearchOutcome = { hits: [], degraded: false, warnings: [] };

export default function UnifiedSearchDialog({ open, onClose, onSelect }: Props) {
  const [query, setQuery] = useState("");
  const [outcome, setOutcome] = useState<UnifiedSearchOutcome>(EMPTY);
  const [resultQuery, setResultQuery] = useState("");
  const [loading, setLoading] = useState(false);
  const [hasPartial, setHasPartial] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (!open) return;
    const timer = window.setTimeout(() => inputRef.current?.focus(), 0);
    return () => window.clearTimeout(timer);
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const trimmed = query.trim();
    setOutcome(EMPTY);
    setResultQuery("");
    setHasPartial(false);
    setError(null);
    if (!trimmed) {
      setLoading(false);
      return;
    }
    const controller = new AbortController();
    let cancelled = false;
    setLoading(true);
    const timer = window.setTimeout(() => {
      void searchAllProgressively(trimmed, { limit: 30 }, {
        signal: controller.signal,
        onProgress: partial => {
          if (cancelled) return;
          setOutcome(partial);
          setResultQuery(trimmed);
          setHasPartial(true);
        },
      })
        .then(result => {
          if (cancelled) return;
          setOutcome(result);
          setResultQuery(trimmed);
        })
        .catch(reason => {
          // Keep already-published lexical hits instead of replacing them
          // with a misleading disabled/empty search result.
          if (!cancelled) setError(String(reason));
        })
        .finally(() => {
          if (!cancelled) setLoading(false);
        });
    }, 180);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
      controller.abort();
    };
  }, [open, query]);

  useEffect(() => {
    if (!open) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [open, onClose]);

  const current = resultQuery === query.trim();
  const groups = useMemo(() => ({
    knowledge: current ? outcome.hits.filter(hit => hit.corpus === "knowledge") : [],
    session: current ? outcome.hits.filter(hit => hit.corpus === "session") : [],
  }), [outcome.hits, current]);

  if (!open) return null;

  return (
    <div className="absolute inset-0 z-50 flex items-start justify-center bg-black/20 px-6 pt-16 backdrop-blur-[1px]" onMouseDown={onClose}>
      <div
        className="flex max-h-[72vh] w-full max-w-3xl flex-col overflow-hidden rounded-xl border border-gray-200 bg-white shadow-2xl dark:border-gray-700 dark:bg-gray-850"
        onMouseDown={event => event.stopPropagation()}
      >
        <div className="flex items-center gap-3 border-b border-gray-200 px-4 py-3 dark:border-gray-700">
          <Search className="h-4 w-4 flex-shrink-0 text-gray-400" />
          <input
            ref={inputRef}
            value={query}
            onChange={event => setQuery(event.target.value)}
            placeholder="搜索知识和 AI 对话记录..."
            className="min-w-0 flex-1 bg-transparent text-sm text-gray-900 outline-none placeholder:text-gray-400 dark:text-gray-100"
          />
          {loading && <Loader2 className="h-4 w-4 animate-spin text-gray-400" />}
          <button type="button" onClick={onClose} className="rounded p-1 text-gray-400 hover:bg-gray-100 hover:text-gray-700 dark:hover:bg-gray-700 dark:hover:text-gray-200" aria-label="关闭搜索">
            <X className="h-4 w-4" />
          </button>
        </div>

        {query.trim() && current && !loading && !error && outcome.semantic_enabled === false && !outcome.degraded && (
          <div className="border-b border-sky-200 bg-sky-50 px-4 py-2 text-xs text-sky-700 dark:border-sky-900 dark:bg-sky-950/30 dark:text-sky-200">
            关键词检索模式 · 可在设置中启用语义搜索获得语义召回
          </div>
        )}
        {loading && current && hasPartial && outcome.semantic_enabled === true && (
          <div role="status" className="border-b border-gray-200 px-4 py-2 text-xs text-gray-500 dark:border-gray-700 dark:text-gray-400">
            {outcome.hits.length > 0 ? "关键词结果已显示，正在补充语义结果…" : "关键词暂无匹配，正在检索语义相关内容…"}
          </div>
        )}
        {current && outcome.degraded && outcome.warnings.length > 0 && (
          <div className="border-b border-amber-200 bg-amber-50 px-4 py-2 text-xs text-amber-700 dark:border-amber-900 dark:bg-amber-950/30 dark:text-amber-200">
            {outcome.warnings[0]}
          </div>
        )}
        {error && (
          <div className="border-b border-red-200 bg-red-50 px-4 py-2 text-xs text-red-600 dark:border-red-900 dark:bg-red-950/30 dark:text-red-300">
            {error}
          </div>
        )}

        <div className="min-h-0 flex-1 overflow-auto p-2">
          {!query.trim() ? (
            <div className="px-3 py-10 text-center text-sm text-gray-400">输入关键词，可同时检索知识与 AI 对话记录</div>
          ) : !loading && current && outcome.hits.length === 0 && !error ? (
            <div className="px-3 py-10 text-center text-sm text-gray-400">没有找到相关结果</div>
          ) : (
            <>
              {groups.knowledge.length > 0 && (
                <ResultGroup title="知识" icon={<FileText className="h-3.5 w-3.5" />} hits={groups.knowledge} onSelect={onSelect} />
              )}
              {groups.session.length > 0 && (
                <ResultGroup title="AI 对话" icon={<MessageSquareText className="h-3.5 w-3.5" />} hits={groups.session} onSelect={onSelect} />
              )}
            </>
          )}
        </div>
      </div>
    </div>
  );
}

function ResultGroup({ title, icon, hits, onSelect }: {
  title: string;
  icon: React.ReactNode;
  hits: UnifiedSearchHit[];
  onSelect: (hit: UnifiedSearchHit) => void;
}) {
  return (
    <section className="mb-2 last:mb-0">
      <div className="flex items-center gap-1.5 px-2 py-1.5 text-[11px] font-semibold uppercase tracking-wide text-gray-400">
        {icon}<span>{title}</span><span className="font-normal">{hits.length}</span>
      </div>
      <div className="space-y-1">
        {hits.map(hit => (
          <button key={`${hit.corpus}:${hit.entity_id}`} type="button" onClick={() => onSelect(hit)}
            className="block w-full rounded-lg px-3 py-2.5 text-left transition-colors hover:bg-gray-50 dark:hover:bg-gray-800">
            <div className="flex items-center gap-2">
              <span className="min-w-0 flex-1 truncate text-sm font-medium text-gray-900 dark:text-gray-100">{hit.title}</span>
              <span className="flex-shrink-0 rounded bg-gray-100 px-1.5 py-0.5 text-[10px] text-gray-500 dark:bg-gray-700 dark:text-gray-300">
                {hit.match_types.join(" + ")}
              </span>
            </div>
            {hit.snippet && <div className="mt-1 line-clamp-2 text-xs leading-5 text-gray-500 dark:text-gray-400">{hit.snippet}</div>}
          </button>
        ))}
      </div>
    </section>
  );
}
