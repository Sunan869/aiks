import { useEffect, useState } from "react";
import { Database, GitFork, Library, MessagesSquare } from "lucide-react";
import { getApi } from "../api/client";
import type { KnowledgeDetail, WorkspaceMode } from "../api/types";
import WorkbenchHost from "../components/WorkbenchHost";

type WorkspaceSection = "knowledge" | "session" | "database" | "graph";

interface Props {
  knowledgeId?: string;
}

const tabs: Array<{ key: WorkspaceSection; label: string; icon: typeof Library }> = [
  { key: "knowledge", label: "知识", icon: Library },
  { key: "session", label: "原始会话", icon: MessagesSquare },
  { key: "database", label: "数据库", icon: Database },
  { key: "graph", label: "图谱", icon: GitFork },
];

export default function KnowledgeWorkspacePage({ knowledgeId }: Props) {
  const [section, setSection] = useState<WorkspaceSection>("knowledge");
  const [detail, setDetail] = useState<KnowledgeDetail | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!knowledgeId) {
      setDetail(null);
      return;
    }
    let cancelled = false;
    getApi().getKnowledgeDetail(knowledgeId)
      .then(item => { if (!cancelled) setDetail(item); })
      .catch(e => { if (!cancelled) setError(String(e)); });
    return () => { cancelled = true; };
  }, [knowledgeId]);

  const select = async (next: WorkspaceSection) => {
    setSection(next);
    setError(null);
    try {
      if (next === "database") await getApi().showWorkbenchDatabase();
      if (next === "graph") await getApi().showWorkbenchGraph();
    } catch (e) {
      setError(String(e));
    }
  };

  const mode: WorkspaceMode = section === "session" ? "session" : "knowledge";
  const boundDocId = section === "knowledge" ? detail?.siyuan_doc_id : null;

  return (
    <div className="flex h-full min-h-0 flex-col p-6">
      <div className="mb-4 flex items-start justify-between gap-4">
        <div>
          <div className="flex items-center gap-2">
            <h1 className="text-lg font-semibold text-gray-900 dark:text-gray-100">知识库</h1>
            <span className="rounded bg-blue-50 px-1.5 py-0.5 text-[10px] font-semibold text-blue-700 dark:bg-blue-900/30 dark:text-blue-300">V4.1 Workbench</span>
          </div>
          <p className="mt-1 text-sm text-gray-500">SiYuan 作为正文 Master；AIKS 负责采集、提炼、搜索、来源关系和工作台编排。</p>
        </div>
      </div>

      <div className="mb-4 flex gap-1 border-b border-gray-200 pb-3 dark:border-gray-700">
        {tabs.map(tab => {
          const Icon = tab.icon;
          return (
            <button
              key={tab.key}
              type="button"
              onClick={() => void select(tab.key)}
              className={`inline-flex items-center gap-1.5 rounded-md px-3 py-1.5 text-xs font-medium transition-colors ${section === tab.key
                ? "bg-gray-900 text-white dark:bg-gray-100 dark:text-gray-900"
                : "text-gray-500 hover:bg-gray-100 hover:text-gray-800 dark:hover:bg-gray-800 dark:hover:text-gray-200"}`}
            >
              <Icon className="h-3.5 w-3.5" />
              {tab.label}
            </button>
          );
        })}
      </div>

      {error && <div className="mb-3 rounded-lg bg-red-50 px-3 py-2 text-xs text-red-600 dark:bg-red-950/30 dark:text-red-300">{error}</div>}

      {knowledgeId && detail && !detail.siyuan_doc_id && section === "knowledge" ? (
        <div className="mb-3 rounded-lg border border-amber-200 bg-amber-50 px-4 py-3 text-sm text-amber-800 dark:border-amber-900 dark:bg-amber-950/30 dark:text-amber-200">
          这条知识尚未绑定 Canonical SiYuan 文档，将由 V4.1 内容迁移流程处理；旧 SQLite 正文不会在默认详情页继续作为编辑 Master。
        </div>
      ) : null}

      {section === "knowledge" || section === "session" ? (
        <WorkbenchHost mode={mode} docId={boundDocId} />
      ) : (
        <div className="flex min-h-0 flex-1 items-center justify-center rounded-xl border border-gray-200 bg-white p-8 text-center dark:border-gray-700 dark:bg-gray-800">
          <div>
            <p className="text-sm font-medium text-gray-800 dark:text-gray-200">{section === "database" ? "SiYuan 数据库视图已打开" : "SiYuan 图谱已打开"}</p>
            <p className="mt-2 text-xs text-gray-500">该能力由原生 Workbench 提供，AIKS 不重复实现第二套视图。</p>
          </div>
        </div>
      )}
    </div>
  );
}
