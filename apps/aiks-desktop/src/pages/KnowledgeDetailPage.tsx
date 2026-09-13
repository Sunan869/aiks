import { useState, useEffect, useCallback } from "react";

interface KnowledgeDetail {
  id: string;
  session_id: number;
  project_name: string | null;
  title: string;
  category: string;
  summary: string;
  content: string;
  tags: string;
  confidence: number;
  created_at: string;
  updated_at: string;
  source: string;
  session_external_id: string;
  session_title: string | null;
  chunks: Array<{
    id: string;
    heading: string | null;
    chunk_index: number;
    token_count: number;
    text: string;
    has_embedding: boolean;
  }>;
}

interface Props {
  knowledgeId: string;
  onBack: () => void;
  onViewSession: (sessionId: number) => void;
}

function parseTags(tags: string): string[] {
  try { return JSON.parse(tags); } catch { return tags ? [tags] : []; }
}

const CATEGORY_LABELS: Record<string, string> = {
  troubleshooting: "故障排查", architecture: "架构设计", implementation: "实现方案",
  configuration: "配置管理", research: "技术探索", decision: "决策记录", general: "通用",
};

const CATEGORY_COLORS: Record<string, string> = {
  troubleshooting: "bg-red-50 text-red-700 dark:bg-red-900/30 dark:text-red-400",
  architecture: "bg-purple-50 text-purple-700",
  implementation: "bg-blue-50 text-blue-700",
  configuration: "bg-yellow-50 text-yellow-700",
  research: "bg-teal-50 text-teal-700",
  decision: "bg-indigo-50 text-indigo-700",
  general: "bg-gray-100 text-gray-600",
};

export default function KnowledgeDetailPage({ knowledgeId, onBack, onViewSession }: Props) {
  const [data, setData] = useState<KnowledgeDetail | null>(null);
  const [loading, setLoading] = useState(true);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
        const { invoke } = await import("@tauri-apps/api/core");
        const d = await invoke<KnowledgeDetail>("get_knowledge_detail", { knowledgeId });
        setData(d);
      } else {
        // Mock
        setData({
          id: knowledgeId,
          session_id: 42,
          project_name: "AIKS",
          title: "PowerShell NativeCommandError 修复",
          category: "troubleshooting",
          summary: "PowerShell 5.1 中，当原生进程写入 stderr 时，ErrorActionPreference=Stop 会导致 ErrorRecord 触发，使 cargo build 等正常完成的命令误报失败。",
          content: "## 问题\n\nPowerShell 5.1 将 native 进程的 stderr 输出转换为 ErrorRecord 对象。当 `$ErrorActionPreference = 'Stop'` 时，这些 ErrorRecord 会触发终止。\n\n## 根因\n\ncargo/rustc 即使编译成功也会将 warnings 输出到 stderr。\n\n## 解决方案\n\n在 Invoke-NativeCommand 函数中，临时将 ErrorActionPreference 设为 Continue，执行命令后恢复：\n\n```powershell\n$previousErrorActionPreference = $ErrorActionPreference\ntry {\n    $ErrorActionPreference = 'Continue'\n    & $FilePath @Arguments\n    $exitCode = $LASTEXITCODE\n} finally {\n    $ErrorActionPreference = $previousErrorActionPreference\n}\n```",
          tags: '["PowerShell", "Cargo", "Build"]',
          confidence: 0.92,
          created_at: new Date().toISOString(),
          updated_at: new Date().toISOString(),
          source: "opencode",
          session_external_id: "ses_f717",
          session_title: "AIKS Build Pipeline 修复",
          chunks: [
            { id: "chunk1", heading: null, chunk_index: 0, token_count: 380, text: "PowerShell 5.1 中，当原生进程写入 stderr...", has_embedding: false },
          ],
        });
      }
    } finally {
      setLoading(false);
    }
  }, [knowledgeId]);

  useEffect(() => { load(); }, [load]);

  if (loading) return (
    <div className="p-6 flex items-center justify-center h-48 text-gray-400 text-sm">加载中...</div>
  );

  if (!data) return <div className="p-6 text-gray-400">未找到知识条目: {knowledgeId}</div>;

  const tags = parseTags(data.tags);

  return (
    <div className="p-6 max-w-3xl">
      <div className="flex items-center gap-2 mb-5">
        <button onClick={onBack} className="text-sm text-blue-500 hover:text-blue-700">← 返回</button>
        <span className="text-gray-300">/</span>
        <span className="text-sm text-gray-600 dark:text-gray-400">知识详情</span>
      </div>

      {/* Header */}
      <div className="bg-white dark:bg-gray-800 rounded border border-gray-200 dark:border-gray-700 p-5 mb-5">
        <div className="flex items-start gap-3">
          <div className="flex-1">
            <h1 className="text-lg font-semibold text-gray-900 dark:text-gray-100">{data.title}</h1>
            <div className="flex items-center gap-2 mt-2 flex-wrap">
              <span className={`text-xs px-2 py-0.5 rounded font-medium ${CATEGORY_COLORS[data.category] ?? CATEGORY_COLORS.general}`}>
                {CATEGORY_LABELS[data.category] ?? data.category}
              </span>
              {data.project_name && (
                <span className="text-xs px-2 py-0.5 bg-gray-100 dark:bg-gray-700 text-gray-600 dark:text-gray-400 rounded">
                  {data.project_name}
                </span>
              )}
              {tags.map(tag => (
                <span key={tag} className="text-xs px-2 py-0.5 bg-gray-100 dark:bg-gray-700 text-gray-500 rounded">
                  {tag}
                </span>
              ))}
              <span className="text-xs text-gray-400">{(data.confidence * 100).toFixed(0)}% 置信度</span>
            </div>
          </div>
        </div>

        <p className="mt-3 text-sm text-gray-600 dark:text-gray-400 leading-relaxed">{data.summary}</p>

        {/* Source tracing */}
        <div className="mt-4 pt-3 border-t border-gray-100 dark:border-gray-700 flex items-center justify-between">
          <div className="text-xs text-gray-400">
            来源：<span className="font-mono">{data.session_external_id}</span>
            {data.session_title && <span className="ml-1">· {data.session_title}</span>}
          </div>
          <button
            onClick={() => onViewSession(data.session_id)}
            className="text-xs px-2.5 py-1 border border-gray-200 dark:border-gray-700 rounded hover:bg-gray-50 dark:hover:bg-gray-700 text-gray-600 dark:text-gray-400"
          >
            查看原始工作记录
          </button>
        </div>
      </div>

      {/* Content */}
      <div className="bg-white dark:bg-gray-800 rounded border border-gray-200 dark:border-gray-700 p-5 mb-5">
        <h2 className="text-sm font-medium text-gray-700 dark:text-gray-300 mb-3">详细内容</h2>
        <div className="prose prose-sm dark:prose-invert max-w-none">
          <pre className="whitespace-pre-wrap text-xs text-gray-700 dark:text-gray-300 leading-relaxed font-sans">
            {data.content}
          </pre>
        </div>
      </div>

      {/* Embedding chunks */}
      {data.chunks.length > 0 && (
        <div className="bg-white dark:bg-gray-800 rounded border border-gray-200 dark:border-gray-700 p-4">
          <h2 className="text-sm font-medium text-gray-700 dark:text-gray-300 mb-3">
            向量切片 ({data.chunks.length} 块)
          </h2>
          {data.chunks.map(chunk => (
            <div key={chunk.id} className="py-2 border-b border-gray-100 dark:border-gray-700 last:border-0">
              <div className="flex items-center gap-3 text-xs text-gray-400 mb-1">
                <span>#{chunk.chunk_index + 1}</span>
                <span>{chunk.token_count} tokens</span>
                {chunk.has_embedding ? (
                  <span className="text-green-500">✓ 已向量化</span>
                ) : (
                  <span className="text-gray-300">— 未向量化</span>
                )}
              </div>
              <p className="text-xs text-gray-500 truncate">{chunk.text}</p>
            </div>
          ))}
        </div>
      )}

      {/* Metadata */}
      <div className="mt-3 text-xs text-gray-400 flex gap-4">
        <span>创建：{new Date(data.created_at).toLocaleDateString("zh-CN")}</span>
        <span>更新：{new Date(data.updated_at).toLocaleDateString("zh-CN")}</span>
      </div>
    </div>
  );
}
