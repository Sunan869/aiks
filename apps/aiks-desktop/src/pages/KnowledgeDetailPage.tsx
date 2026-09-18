import { useCallback, useEffect, useState } from "react";
import { Archive, Bot, ExternalLink, FilePenLine, Loader2, Pencil, RotateCcw, Star } from "lucide-react";
import { getApi } from "../api/client";
import type { KnowledgeDetail } from "../api/types";
import KnowledgeEditor from "../components/KnowledgeEditor";

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

export default function KnowledgeDetailPage({ knowledgeId, onBack, onViewSession }: Props) {
  const [data, setData] = useState<KnowledgeDetail | null>(null);
  const [loading, setLoading] = useState(true);
  const [editing, setEditing] = useState(false);
  const [action, setAction] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      setData(await getApi().getKnowledgeDetail(knowledgeId));
    } finally {
      setLoading(false);
    }
  }, [knowledgeId]);

  useEffect(() => { void load(); }, [load]);

  const runAction = async (name: string, fn: () => Promise<KnowledgeDetail>) => {
    setAction(name);
    setMessage(null);
    try {
      setData(await fn());
    } catch (e) {
      setMessage(`操作失败：${String(e)}`);
    } finally {
      setAction(null);
    }
  };

  const publish = async () => {
    if (!data) return;
    setAction("publish");
    setMessage(null);
    try {
      const result = await getApi().publishKnowledge(data.id);
      const labels: Record<string, string> = {
        created: "已发布到 SiYuan",
        updated: "已更新 SiYuan 文档",
        unchanged: "SiYuan 中已是最新版本",
        conflict: "检测到 SiYuan 端人工修改，未覆盖远端内容",
      };
      setMessage(labels[result.outcome] ?? `SiYuan 发布结果：${result.outcome}`);
    } catch (e) {
      setMessage(`发布失败：${String(e)}`);
    } finally {
      setAction(null);
    }
  };

  if (loading) return <div className="flex h-48 items-center justify-center p-6 text-sm text-gray-400">加载中...</div>;
  if (!data) return <div className="p-6 text-gray-400">未找到知识条目：{knowledgeId}</div>;

  const tags = parseTags(data.tags);

  return (
    <div className="p-6">
      <div className="mb-5 flex items-center gap-2">
        <button onClick={onBack} className="text-sm text-blue-500 hover:text-blue-700">← 返回</button>
        <span className="text-gray-300">/</span>
        <span className="text-sm text-gray-600 dark:text-gray-400">知识详情</span>
      </div>

      <div className="mb-5 rounded-lg border border-gray-200 bg-white p-5 dark:border-gray-700 dark:bg-gray-800">
        <div className="flex items-start justify-between gap-5">
          <div className="min-w-0 flex-1">
            <div className="mb-2 flex flex-wrap items-center gap-2">
              {data.source_type === "manual" ? (
                <span className="inline-flex items-center gap-1 rounded bg-emerald-50 px-2 py-1 text-xs font-medium text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-300"><FilePenLine className="h-3.5 w-3.5" />手动创建</span>
              ) : (
                <span className="inline-flex items-center gap-1 rounded bg-blue-50 px-2 py-1 text-xs font-medium text-blue-700 dark:bg-blue-900/30 dark:text-blue-300"><Bot className="h-3.5 w-3.5" />AI 提炼</span>
              )}
              <span className="rounded bg-gray-100 px-2 py-1 text-xs text-gray-600 dark:bg-gray-700 dark:text-gray-300">{CATEGORY_LABELS[data.category] ?? data.category}</span>
              {data.managed_by === "user" && data.source_type === "conversation" && <span className="rounded bg-amber-50 px-2 py-1 text-xs text-amber-700 dark:bg-amber-900/30 dark:text-amber-300">用户管理 · AI 不再覆盖</span>}
              {data.status === "archived" && <span className="rounded bg-gray-100 px-2 py-1 text-xs text-gray-500">已归档</span>}
            </div>
            <h1 className="text-xl font-semibold text-gray-900 dark:text-gray-100">{data.title}</h1>
            {data.summary && <p className="mt-3 text-sm leading-6 text-gray-500 dark:text-gray-400">{data.summary}</p>}
          </div>

          <div className="flex flex-wrap justify-end gap-2">
            <button onClick={() => setEditing(true)} className="inline-flex items-center gap-1.5 rounded-md border border-gray-200 px-3 py-1.5 text-xs text-gray-600 hover:bg-gray-50 dark:border-gray-700 dark:text-gray-300 dark:hover:bg-gray-700"><Pencil className="h-3.5 w-3.5" />编辑</button>
            <button
              onClick={() => void runAction("favorite", () => getApi().setKnowledgeFavorite(data.id, !data.is_favorite))}
              disabled={action !== null}
              className={`inline-flex items-center gap-1.5 rounded-md border px-3 py-1.5 text-xs ${data.is_favorite ? "border-amber-200 bg-amber-50 text-amber-700" : "border-gray-200 text-gray-600 dark:border-gray-700 dark:text-gray-300"}`}
            >
              {action === "favorite" ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Star className="h-3.5 w-3.5" fill={data.is_favorite ? "currentColor" : "none"} />}
              {data.is_favorite ? "已收藏" : "收藏"}
            </button>
            {data.status === "archived" ? (
              <button onClick={() => void runAction("restore", () => getApi().restoreKnowledge(data.id))} disabled={action !== null} className="inline-flex items-center gap-1.5 rounded-md border border-gray-200 px-3 py-1.5 text-xs text-gray-600 dark:border-gray-700 dark:text-gray-300"><RotateCcw className="h-3.5 w-3.5" />恢复</button>
            ) : (
              <button onClick={() => void runAction("archive", () => getApi().archiveKnowledge(data.id))} disabled={action !== null} className="inline-flex items-center gap-1.5 rounded-md border border-gray-200 px-3 py-1.5 text-xs text-gray-600 dark:border-gray-700 dark:text-gray-300"><Archive className="h-3.5 w-3.5" />归档</button>
            )}
            <button onClick={() => void publish()} disabled={action !== null || data.status === "archived"} className="inline-flex items-center gap-1.5 rounded-md border border-violet-200 bg-violet-50 px-3 py-1.5 text-xs text-violet-700 hover:bg-violet-100 disabled:opacity-40 dark:border-violet-800 dark:bg-violet-900/20 dark:text-violet-300">
              {action === "publish" ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <ExternalLink className="h-3.5 w-3.5" />}
              发布到 SiYuan
            </button>
          </div>
        </div>

        <div className="mt-4 flex flex-wrap gap-1.5">
          {data.project_name && <span className="rounded bg-gray-100 px-2 py-0.5 text-xs text-gray-500 dark:bg-gray-700">项目：{data.project_name}</span>}
          {tags.map(tag => <span key={tag} className="rounded bg-gray-100 px-2 py-0.5 text-xs text-gray-500 dark:bg-gray-700">#{tag}</span>)}
        </div>

        <div className="mt-4 border-t border-gray-100 pt-3 dark:border-gray-700">
          {data.source_type === "manual" ? (
            <div className="flex items-center gap-2 text-xs text-gray-400"><FilePenLine className="h-3.5 w-3.5" />这条知识由你直接在 AIKS 中创建，没有绑定外部工作记录。</div>
          ) : (
            <div className="flex items-center justify-between gap-3">
              <div className="text-xs text-gray-400">来源：<span className="font-mono">{data.session_external_id}</span>{data.session_title && <span className="ml-1">· {data.session_title}</span>}</div>
              {data.session_id !== null && <button onClick={() => onViewSession(data.session_id as number)} className="rounded border border-gray-200 px-2.5 py-1 text-xs text-gray-600 hover:bg-gray-50 dark:border-gray-700 dark:text-gray-300 dark:hover:bg-gray-700">查看原始工作记录</button>}
            </div>
          )}
        </div>
      </div>

      {message && <div className={`mb-4 rounded-md border px-4 py-2 text-xs ${message.includes("失败") ? "border-red-200 bg-red-50 text-red-700" : message.includes("冲突") ? "border-amber-200 bg-amber-50 text-amber-700" : "border-green-200 bg-green-50 text-green-700"}`}>{message}</div>}

      <div className="mb-5 rounded-lg border border-gray-200 bg-white p-5 dark:border-gray-700 dark:bg-gray-800">
        <div className="mb-3 flex items-center justify-between">
          <h2 className="text-sm font-medium text-gray-700 dark:text-gray-300">知识内容</h2>
          <span className="text-[10px] text-gray-400">Markdown</span>
        </div>
        <pre className="whitespace-pre-wrap font-sans text-sm leading-7 text-gray-700 dark:text-gray-300">{data.content}</pre>
      </div>

      <div className="flex gap-4 text-xs text-gray-400">
        <span>创建：{new Date(data.created_at).toLocaleString("zh-CN")}</span>
        <span>更新：{new Date(data.updated_at).toLocaleString("zh-CN")}</span>
        <span>{data.source_type === "manual" ? "本地手工知识" : `${Math.round(data.confidence * 100)}% AI 置信度`}</span>
      </div>

      <KnowledgeEditor
        open={editing}
        initial={data}
        onClose={() => setEditing(false)}
        onSaved={saved => {
          setEditing(false);
          setData(saved);
          setMessage("知识已保存");
        }}
      />
    </div>
  );
}
