import { useEffect, useState } from "react";
import { Database, FileText, GitFork, Search } from "lucide-react";
import { getApi } from "../api/client";
import type { KnowledgeDetail, UnifiedSearchHit } from "../api/types";
import type { WorkspaceMode } from "../api/workbench";
import UnifiedSearchDialog from "../components/UnifiedSearchDialog";
import WorkbenchHost from "../components/WorkbenchHost";
import {
  resolveWorkbenchSurface,
  type WorkbenchMainMode,
} from "../knowledge-workspace";

interface Props {
  knowledgeId?: string;
  workspaceMode?: WorkspaceMode;
  workbenchDocId?: string | null;
  onOpenKnowledge?: (knowledgeId: string) => void;
  onOpenSession?: (sessionId: number, siyuanDocId: string | null) => void;
}

const mainModes: Array<{
  key: WorkbenchMainMode;
  label: string;
  icon: typeof FileText;
}> = [
  { key: "document", label: "文档", icon: FileText },
  { key: "database", label: "数据库", icon: Database },
  { key: "graph", label: "图谱", icon: GitFork },
];

export default function KnowledgeWorkspacePage({
  knowledgeId,
  workspaceMode = "knowledge",
  workbenchDocId = null,
  onOpenKnowledge,
  onOpenSession,
}: Props) {
  const [mainMode, setMainMode] = useState<WorkbenchMainMode>("document");
  const [detail, setDetail] = useState<KnowledgeDetail | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [searchOpen, setSearchOpen] = useState(false);

  useEffect(() => {
    setMainMode("document");
    setError(null);
  }, [knowledgeId, workspaceMode, workbenchDocId]);

  useEffect(() => {
    if (!knowledgeId) {
      setDetail(null);
      return;
    }

    let cancelled = false;
    setError(null);
    getApi().getKnowledgeDetail(knowledgeId)
      .then(item => {
        if (!cancelled) setDetail(item);
      })
      .catch(e => {
        if (!cancelled) setError(String(e));
      });

    return () => {
      cancelled = true;
    };
  }, [knowledgeId]);

  const surface = resolveWorkbenchSurface(mainMode, workspaceMode);
  const boundDocId = mainMode === "document"
    ? workspaceMode === "session"
      ? workbenchDocId
      : detail?.siyuan_doc_id
    : null;

  const selectSearchHit = async (hit: UnifiedSearchHit) => {
    setSearchOpen(false);
    setError(null);
    try {
      if (hit.corpus === "knowledge") {
        if (onOpenKnowledge) {
          onOpenKnowledge(hit.entity_id);
          return;
        }
        if (hit.siyuan_doc_id) {
          await getApi().openSiyuanDocument(hit.siyuan_doc_id, "knowledge");
        }
        return;
      }

      const sessionId = Number(hit.entity_id);
      if (onOpenSession && Number.isFinite(sessionId)) {
        onOpenSession(sessionId, hit.siyuan_doc_id);
        return;
      }
      if (hit.siyuan_doc_id) {
        await getApi().openSiyuanDocument(hit.siyuan_doc_id, "session");
      }
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <div className="relative flex h-full min-h-0 flex-col bg-white dark:bg-gray-900">
      <div className="flex h-12 flex-shrink-0 items-center gap-3 border-b border-gray-200 px-3 dark:border-gray-700 dark:bg-gray-800">
        <div className="flex min-w-0 items-center gap-2">
          <span className="text-sm font-semibold text-gray-900 dark:text-gray-100">知识库</span>
          {workspaceMode === "session" && (
            <span className="whitespace-nowrap rounded bg-gray-100 px-1.5 py-0.5 text-[10px] font-medium text-gray-500 dark:bg-gray-700 dark:text-gray-300">
              AI 对话记录 · 只读
            </span>
          )}
        </div>

        <button
          type="button"
          onClick={() => setSearchOpen(true)}
          className="ml-2 flex min-w-0 max-w-md flex-1 items-center gap-2 rounded-md border border-gray-200 bg-gray-50 px-3 py-1.5 text-left text-xs text-gray-400 transition-colors hover:border-gray-300 hover:bg-white dark:border-gray-700 dark:bg-gray-900 dark:hover:border-gray-600 dark:hover:bg-gray-800"
          title="AIKS 统一搜索"
        >
          <Search className="h-3.5 w-3.5 flex-shrink-0" />
          <span className="truncate">搜索知识和 AI 对话记录...</span>
        </button>

        <div className="ml-auto flex flex-shrink-0 items-center gap-1 rounded-md bg-gray-100 p-0.5 dark:bg-gray-900">
          {mainModes.map(item => {
            const Icon = item.icon;
            const active = mainMode === item.key;
            return (
              <button
                key={item.key}
                type="button"
                onClick={() => {
                  setError(null);
                  setMainMode(item.key);
                }}
                className={`inline-flex items-center gap-1.5 rounded px-2.5 py-1.5 text-xs font-medium transition-colors ${active
                  ? "bg-white text-gray-900 shadow-sm dark:bg-gray-700 dark:text-gray-100"
                  : "text-gray-500 hover:text-gray-800 dark:text-gray-400 dark:hover:text-gray-200"}`}
              >
                <Icon className="h-3.5 w-3.5" />
                {item.label}
              </button>
            );
          })}
        </div>
      </div>

      {error && (
        <div className="mx-3 mt-3 flex-shrink-0 rounded-lg bg-red-50 px-3 py-2 text-xs text-red-600 dark:bg-red-950/30 dark:text-red-300">
          {error}
        </div>
      )}

      {knowledgeId && detail && !detail.siyuan_doc_id && mainMode === "document" && workspaceMode === "knowledge" ? (
        <div className="mx-3 mt-3 flex-shrink-0 rounded-lg border border-amber-200 bg-amber-50 px-4 py-3 text-xs text-amber-800 dark:border-amber-900 dark:bg-amber-950/30 dark:text-amber-200">
          这条知识尚未绑定 Canonical SiYuan 文档，将由内容迁移流程处理。
        </div>
      ) : null}

      <div className="flex min-h-0 flex-1 p-3">
        <WorkbenchHost surface={surface} docId={boundDocId} />
      </div>

      <UnifiedSearchDialog
        open={searchOpen}
        onClose={() => setSearchOpen(false)}
        onSelect={hit => void selectSearchHit(hit)}
      />
    </div>
  );
}
