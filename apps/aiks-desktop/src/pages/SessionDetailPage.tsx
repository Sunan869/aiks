import { useCallback, useEffect, useState } from "react";
import { useSourceName } from "../ProviderCatalog";

interface SessionDetailData {
  session: {
    id: number;
    source: string;
    session_id: string;
    title: string | null;
    project_name: string | null;
    project_path: string | null;
    updated_at: string | null;
    content_hash: string | null;
  };
  pipeline_run: {
    run_id: string;
    status: string;
    current_stage: string | null;
    pipeline_version: string;
    started_at: string | null;
    finished_at: string | null;
    error_stage: string | null;
    error_message: string | null;
  } | null;
  chunks: Array<{
    index: number;
    message_start: number;
    message_end: number;
    token_count: number;
  }>;
  knowledge: Array<{
    id: string;
    title: string;
    category: string;
    summary: string;
    confidence: number;
    created_at: string;
  }>;
}

interface Props {
  sessionId: number;
  onBack: () => void;
  onViewKnowledge: (id: string) => void;
  onViewPipeline: (runId: string) => void;
  onViewRawConversation: (docId: string | null) => void;
}

const CATEGORY_LABELS: Record<string, string> = {
  troubleshooting: "故障排查",
  architecture: "架构设计",
  implementation: "实现方案",
  configuration: "配置管理",
  research: "技术探索",
  decision: "决策记录",
  general: "通用",
};

export default function SessionDetailPage({
  sessionId,
  onBack,
  onViewKnowledge,
  onViewPipeline,
  onViewRawConversation,
}: Props) {
  const formatSourceName = useSourceName();
  const [data, setData] = useState<SessionDetailData | null>(null);
  const [sessionDocId, setSessionDocId] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [triggering, setTriggering] = useState(false);
  const [triggerMsg, setTriggerMsg] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
        const { invoke } = await import("@tauri-apps/api/core");
        const [detail, docId] = await Promise.all([
          invoke<SessionDetailData>("get_session_detail", { sessionId }),
          invoke<string | null>("get_session_workbench_doc_id", { sessionId }),
        ]);
        setData(detail);
        setSessionDocId(docId);
      } else {
        setData({
          session: {
            id: sessionId,
            source: "opencode",
            session_id: `ses_${sessionId.toString(16).padStart(4, "0")}`,
            title: `工作记录 ${sessionId}`,
            project_name: "AIKS",
            project_path: "/home/user/projects/aiks",
            updated_at: new Date(Date.now() - sessionId * 3600 * 1000).toISOString(),
            content_hash: `hash_${sessionId}`,
          },
          pipeline_run: {
            run_id: `run_${sessionId}`,
            status: "READY",
            current_stage: null,
            pipeline_version: "v3",
            started_at: new Date(Date.now() - sessionId * 3600 * 1000).toISOString(),
            finished_at: new Date(Date.now() - sessionId * 1800 * 1000).toISOString(),
            error_stage: null,
            error_message: null,
          },
          chunks: [
            { index: 0, message_start: 0, message_end: 39, token_count: 18230 },
            { index: 1, message_start: 40, message_end: 79, token_count: 21883 },
          ],
          knowledge: [
            {
              id: `kn_${sessionId}_1`,
              title: "PowerShell NativeCommandError 修复",
              category: "troubleshooting",
              summary: "使用错误的错误捕获方式导致 Cargo 警告触发 Stop",
              confidence: 0.92,
              created_at: new Date().toISOString(),
            },
          ],
        });
        setSessionDocId(`mock-session-doc-${sessionId}`);
      }
    } finally {
      setLoading(false);
    }
  }, [sessionId]);

  useEffect(() => {
    void load();
  }, [load]);

  const handleTriggerPipeline = async () => {
    setTriggering(true);
    setTriggerMsg(null);
    try {
      if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
        const { invoke } = await import("@tauri-apps/api/core");
        await invoke("run_pipeline_for_session", { sessionId });
        setTriggerMsg("已提交处理任务，稍后可在处理中心查看进度");
      } else {
        setTriggerMsg("[Mock] 已提交处理任务");
      }
    } catch (e) {
      setTriggerMsg(`错误：${e}`);
    } finally {
      setTriggering(false);
    }
  };

  if (loading) {
    return <div className="flex h-full items-center justify-center p-6 text-sm text-gray-400">加载中...</div>;
  }

  if (!data) return <div className="p-6 text-gray-400">未找到会话</div>;

  const { session, pipeline_run, chunks, knowledge } = data;

  return (
    <div className="flex h-full min-h-0 flex-col p-6">
      <div className="mb-4 flex flex-shrink-0 items-center gap-2">
        <button onClick={onBack} className="text-sm text-blue-500 hover:text-blue-700">← 返回</button>
        <span className="text-gray-300">/</span>
        <span className="text-sm text-gray-600 dark:text-gray-400">工作记录详情</span>
      </div>

      <div className="grid min-h-0 flex-1 grid-cols-[320px_minmax(0,1fr)] gap-4">
        <aside className="min-h-0 space-y-4 overflow-y-auto pr-1">
          <section className="rounded-xl border border-gray-200 bg-white p-4 dark:border-gray-700 dark:bg-gray-800">
            <h1 className="text-sm font-semibold text-gray-900 dark:text-gray-100">
              {session.title || session.session_id}
            </h1>
            <div className="mt-2 space-y-1 text-xs text-gray-500">
              <div className="font-mono break-all">{session.session_id}</div>
              <div>{formatSourceName(session.source)}{session.project_name ? ` · ${session.project_name}` : ""}</div>
              {session.project_path && <div className="break-all font-mono text-gray-400">{session.project_path}</div>}
            </div>
            <div className="mt-4 flex flex-wrap gap-2">
              {pipeline_run && (
                <button
                  onClick={() => onViewPipeline(pipeline_run.run_id)}
                  className="rounded border border-gray-200 px-2.5 py-1.5 text-xs hover:bg-gray-50 dark:border-gray-700 dark:hover:bg-gray-700"
                >
                  查看处理详情
                </button>
              )}
              <button
                onClick={handleTriggerPipeline}
                disabled={triggering}
                className="rounded bg-blue-600 px-2.5 py-1.5 text-xs text-white hover:bg-blue-700 disabled:opacity-50"
              >
                {triggering ? "提交中..." : "重新提炼"}
              </button>
            </div>
            {triggerMsg && <div className="mt-2 text-xs text-blue-600">{triggerMsg}</div>}
          </section>

          {pipeline_run && (
            <section className="rounded-xl border border-gray-200 bg-white p-4 dark:border-gray-700 dark:bg-gray-800">
              <h2 className="mb-2 text-xs font-semibold text-gray-500">处理状态</h2>
              <div className="flex items-center gap-2 text-xs">
                <span className={`font-medium ${{ READY: "text-green-600", PROCESSING: "text-blue-600", FAILED: "text-red-500", RAW_ONLY: "text-gray-500" }[pipeline_run.status] ?? "text-gray-600"}`}>
                  {pipeline_run.status}
                </span>
                {pipeline_run.current_stage && <span className="text-gray-400">{pipeline_run.current_stage}</span>}
              </div>
              {pipeline_run.error_message && (
                <div className="mt-2 text-xs text-red-500">{pipeline_run.error_stage}: {pipeline_run.error_message}</div>
              )}
            </section>
          )}

          {chunks.length > 0 && (
            <section className="rounded-xl border border-gray-200 bg-white p-4 dark:border-gray-700 dark:bg-gray-800">
              <h2 className="mb-2 text-xs font-semibold text-gray-500">LLM 切片 ({chunks.length})</h2>
              <div className="space-y-2">
                {chunks.map(chunk => (
                  <div key={chunk.index} className="text-xs text-gray-500">
                    #{chunk.index + 1} · 消息 {chunk.message_start}–{chunk.message_end} · {chunk.token_count.toLocaleString()} tokens
                  </div>
                ))}
              </div>
            </section>
          )}

          <section className="rounded-xl border border-gray-200 bg-white p-4 dark:border-gray-700 dark:bg-gray-800">
            <h2 className="mb-2 text-xs font-semibold text-gray-500">已提炼知识 ({knowledge.length})</h2>
            {knowledge.length === 0 ? (
              <div className="py-3 text-xs text-gray-400">暂无知识条目</div>
            ) : (
              <div className="space-y-2">
                {knowledge.map(item => (
                  <button
                    key={item.id}
                    type="button"
                    onClick={() => onViewKnowledge(item.id)}
                    className="block w-full rounded-lg border border-gray-100 p-2.5 text-left transition-colors hover:border-blue-300 dark:border-gray-700"
                  >
                    <div className="text-xs font-medium text-gray-900 dark:text-gray-100">{item.title}</div>
                    <div className="mt-1 text-[10px] text-gray-400">
                      {CATEGORY_LABELS[item.category] ?? item.category} · {(item.confidence * 100).toFixed(0)}%
                    </div>
                    <div className="mt-1 line-clamp-2 text-xs text-gray-500">{item.summary}</div>
                  </button>
                ))}
              </div>
            )}
          </section>
        </aside>

        <section className="flex min-h-0 flex-col justify-between rounded-xl border border-gray-200 bg-white p-6 dark:border-gray-700 dark:bg-gray-800">
          <div>
            <div className="flex items-center gap-2">
              <h2 className="text-sm font-semibold text-gray-900 dark:text-gray-100">原始对话</h2>
              <span className="rounded bg-gray-100 px-1.5 py-0.5 text-[10px] font-medium text-gray-500 dark:bg-gray-700">只读</span>
            </div>
            <p className="mt-2 max-w-xl text-sm leading-6 text-gray-500 dark:text-gray-400">
              原始内容统一在知识库的 AI 对话记录中浏览、搜索和引用。工作记录页只保留来源、处理状态和提炼结果等管理信息。
            </p>
            {!sessionDocId && (
              <p className="mt-3 rounded-lg border border-amber-200 bg-amber-50 px-3 py-2 text-xs text-amber-800 dark:border-amber-900 dark:bg-amber-950/30 dark:text-amber-200">
                该工作记录尚未绑定具体 SiYuan 文档，将打开“AI 对话记录”根目录。
              </p>
            )}
          </div>
          <div className="pt-6">
            <button
              type="button"
              onClick={() => onViewRawConversation(sessionDocId)}
              className="rounded-lg bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-700"
            >
              查看原始对话
            </button>
          </div>
        </section>
      </div>
    </div>
  );
}
