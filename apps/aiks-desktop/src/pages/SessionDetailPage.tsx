import { useState, useEffect, useCallback } from "react";

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
}

export default function SessionDetailPage({ sessionId, onBack, onViewKnowledge, onViewPipeline }: Props) {
  const [data, setData] = useState<SessionDetailData | null>(null);
  const [loading, setLoading] = useState(true);
  const [triggering, setTriggering] = useState(false);
  const [triggerMsg, setTriggerMsg] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      // Use tauri invoke if available, else use mock data
      if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
        const { invoke } = await import("@tauri-apps/api/core");
        const d = await invoke<SessionDetailData>("get_session_detail", { sessionId });
        setData(d);
      } else {
        // Mock data
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
            { id: `kn_${sessionId}_1`, title: "PowerShell NativeCommandError 修复", category: "troubleshooting", summary: "使用错误的错误捕获方式导致 Cargo 警告触发 Stop", confidence: 0.92, created_at: new Date().toISOString() },
            { id: `kn_${sessionId}_2`, title: "SiYuan Runtime 集成", category: "implementation", summary: "将 SiYuan 内嵌到 Tauri 应用中的方案", confidence: 0.85, created_at: new Date().toISOString() },
          ],
        });
      }
    } finally {
      setLoading(false);
    }
  }, [sessionId]);

  useEffect(() => { load(); }, [load]);

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

  if (loading) return (
    <div className="p-6 flex items-center justify-center h-48 text-gray-400 text-sm">加载中...</div>
  );

  if (!data) return <div className="p-6 text-gray-400">未找到会话</div>;

  const { session, pipeline_run, chunks, knowledge } = data;

  const CATEGORY_LABELS: Record<string, string> = {
    troubleshooting: "故障排查", architecture: "架构设计", implementation: "实现方案",
    configuration: "配置管理", research: "技术探索", decision: "决策记录", general: "通用",
  };

  return (
    <div className="p-6">
      <div className="flex items-center gap-2 mb-5">
        <button onClick={onBack} className="text-sm text-blue-500 hover:text-blue-700">← 返回</button>
        <span className="text-gray-300">/</span>
        <span className="text-sm text-gray-600 dark:text-gray-400">工作记录详情</span>
      </div>

      {/* Session info */}
      <div className="bg-white dark:bg-gray-800 rounded border border-gray-200 dark:border-gray-700 p-5 mb-5">
        <div className="flex items-start justify-between">
          <div>
            <h1 className="text-base font-semibold text-gray-900 dark:text-gray-100">{session.title || session.session_id}</h1>
            <div className="flex items-center gap-3 mt-1 text-xs text-gray-500">
              <span className="font-mono">{session.session_id}</span>
              <span>·</span>
              <span>{session.source}</span>
              {session.project_name && <><span>·</span><span>{session.project_name}</span></>}
            </div>
            {session.project_path && (
              <div className="text-xs text-gray-400 mt-1 font-mono truncate">{session.project_path}</div>
            )}
          </div>
          <div className="flex gap-2">
            {pipeline_run && (
              <button
                onClick={() => onViewPipeline(pipeline_run.run_id)}
                className="text-xs px-3 py-1.5 border border-gray-200 rounded hover:bg-gray-50 dark:border-gray-700"
              >查看处理详情</button>
            )}
            <button
              onClick={handleTriggerPipeline}
              disabled={triggering}
              className="text-xs px-3 py-1.5 bg-blue-600 hover:bg-blue-700 text-white rounded disabled:opacity-50"
            >{triggering ? "提交中..." : "重新处理"}</button>
          </div>
        </div>
        {triggerMsg && <div className="mt-2 text-xs text-blue-600">{triggerMsg}</div>}
      </div>

      {/* Pipeline status */}
      {pipeline_run && (
        <div className="bg-white dark:bg-gray-800 rounded border border-gray-200 dark:border-gray-700 p-4 mb-5">
          <h2 className="text-sm font-medium text-gray-700 dark:text-gray-300 mb-3">处理状态</h2>
          <div className="flex items-center gap-4 text-sm">
            <span className={`font-medium ${{"READY": "text-green-600", "PROCESSING": "text-blue-600", "FAILED": "text-red-500", "RAW_ONLY": "text-gray-500"}[pipeline_run.status] ?? "text-gray-600"}`}>
              {pipeline_run.status}
            </span>
            {pipeline_run.current_stage && <span className="text-gray-500">当前: {pipeline_run.current_stage}</span>}
          </div>
          {pipeline_run.error_message && (
            <div className="mt-2 text-xs text-red-500">{pipeline_run.error_stage}: {pipeline_run.error_message}</div>
          )}
        </div>
      )}

      {/* LLM Chunks */}
      {chunks.length > 0 && (
        <div className="bg-white dark:bg-gray-800 rounded border border-gray-200 dark:border-gray-700 p-4 mb-5">
          <h2 className="text-sm font-medium text-gray-700 dark:text-gray-300 mb-3">LLM 切片 ({chunks.length} 块)</h2>
          <div className="space-y-2">
            {chunks.map(chunk => (
              <div key={chunk.index} className="flex items-center gap-4 text-xs text-gray-500 py-1 border-b border-gray-50 dark:border-gray-700 last:border-0">
                <span className="font-mono w-12">#{chunk.index + 1}</span>
                <span>消息 {chunk.message_start}–{chunk.message_end}</span>
                <span>{chunk.token_count.toLocaleString()} tokens</span>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Knowledge items */}
      <div className="bg-white dark:bg-gray-800 rounded border border-gray-200 dark:border-gray-700 p-4">
        <h2 className="text-sm font-medium text-gray-700 dark:text-gray-300 mb-3">提炼知识 ({knowledge.length} 条)</h2>
        {knowledge.length === 0 ? (
          <div className="text-sm text-gray-400 text-center py-6">暂无知识条目</div>
        ) : (
          <div className="space-y-2">
            {knowledge.map(item => (
              <div
                key={item.id}
                onClick={() => onViewKnowledge(item.id)}
                className="p-3 rounded border border-gray-100 dark:border-gray-700 hover:border-blue-300 cursor-pointer transition-colors"
              >
                <div className="flex items-start justify-between gap-2">
                  <div className="text-sm font-medium text-gray-900 dark:text-gray-100">{item.title}</div>
                  <div className="flex gap-2 flex-shrink-0">
                    <span className="text-[10px] px-1.5 py-0.5 bg-gray-100 dark:bg-gray-700 text-gray-500 rounded">
                      {CATEGORY_LABELS[item.category] ?? item.category}
                    </span>
                    <span className="text-[10px] text-gray-400">{(item.confidence * 100).toFixed(0)}%</span>
                  </div>
                </div>
                <div className="text-xs text-gray-500 mt-1 line-clamp-2">{item.summary}</div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
