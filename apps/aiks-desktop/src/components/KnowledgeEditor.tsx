import { useEffect, useState } from "react";
import { Eye, Loader2, Save, X } from "lucide-react";
import { getApi } from "../api/client";
import type { KnowledgeDetail } from "../api/types";

const CATEGORIES = [
  ["general", "通用"],
  ["troubleshooting", "故障排查"],
  ["architecture", "架构设计"],
  ["implementation", "实现方案"],
  ["configuration", "配置管理"],
  ["research", "技术探索"],
  ["decision", "决策记录"],
] as const;

function parseTags(tags?: string): string {
  if (!tags) return "";
  try {
    return (JSON.parse(tags) as string[]).join(", ");
  } catch {
    return tags;
  }
}

interface Props {
  open: boolean;
  initial?: KnowledgeDetail | null;
  onClose: () => void;
  onSaved: (item: KnowledgeDetail) => void;
}

export default function KnowledgeEditor({ open, initial, onClose, onSaved }: Props) {
  const [title, setTitle] = useState("");
  const [category, setCategory] = useState("general");
  const [project, setProject] = useState("");
  const [summary, setSummary] = useState("");
  const [content, setContent] = useState("");
  const [tags, setTags] = useState("");
  const [preview, setPreview] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;
    setTitle(initial?.title ?? "");
    setCategory(initial?.category ?? "general");
    setProject(initial?.project_name ?? "");
    setSummary(initial?.summary ?? "");
    setContent(initial?.content ?? "");
    setTags(parseTags(initial?.tags));
    setPreview(false);
    setError(null);
  }, [open, initial]);

  if (!open) return null;

  const save = async () => {
    if (!title.trim()) {
      setError("请输入知识标题");
      return;
    }
    if (!content.trim()) {
      setError("请输入知识内容");
      return;
    }

    setSaving(true);
    setError(null);
    const normalizedTags = tags
      .split(/[,，]/)
      .map(value => value.trim())
      .filter(Boolean);
    try {
      const api = getApi();
      const saved = initial
        ? await api.updateKnowledge(initial.id, {
            title: title.trim(),
            category,
            project_name: project.trim() || null,
            summary: summary.trim(),
            content,
            tags: normalizedTags,
          })
        : await api.createKnowledge({
            title: title.trim(),
            category,
            project_name: project.trim() || null,
            summary: summary.trim(),
            content,
            tags: normalizedTags,
          });
      onSaved(saved);
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-6 backdrop-blur-sm">
      <div className="flex h-[86vh] w-full max-w-5xl flex-col overflow-hidden rounded-xl border border-gray-200 bg-white shadow-2xl dark:border-gray-700 dark:bg-gray-900">
        <div className="flex items-center justify-between border-b border-gray-200 px-5 py-3 dark:border-gray-700">
          <div>
            <h2 className="text-base font-semibold text-gray-900 dark:text-gray-100">
              {initial ? "编辑知识" : "新建知识"}
            </h2>
            <p className="mt-0.5 text-xs text-gray-400">
              {initial?.source_type === "conversation"
                ? "保存后转为用户管理，后续 AI 重新提炼不会覆盖你的修改"
                : "知识直接保存在 AIKS 本地知识库，可按需发布到 SiYuan"}
            </p>
          </div>
          <button type="button" onClick={onClose} className="rounded p-1.5 text-gray-400 hover:bg-gray-100 dark:hover:bg-gray-800">
            <X className="h-4 w-4" />
          </button>
        </div>

        <div className="grid flex-1 min-h-0 grid-cols-[260px_1fr]">
          <div className="space-y-4 overflow-y-auto border-r border-gray-200 p-4 dark:border-gray-700">
            <label className="block text-xs font-medium text-gray-600 dark:text-gray-300">
              标题 <span className="text-red-500">*</span>
              <input value={title} onChange={e => setTitle(e.target.value)} className="mt-1.5 w-full rounded-md border border-gray-200 bg-white px-3 py-2 text-sm outline-none focus:border-blue-400 dark:border-gray-700 dark:bg-gray-800" placeholder="例如：Windows 下 Tauri 编译排错" />
            </label>

            <label className="block text-xs font-medium text-gray-600 dark:text-gray-300">
              分类
              <select value={category} onChange={e => setCategory(e.target.value)} className="mt-1.5 w-full rounded-md border border-gray-200 bg-white px-3 py-2 text-sm dark:border-gray-700 dark:bg-gray-800">
                {CATEGORIES.map(([value, label]) => <option key={value} value={value}>{label}</option>)}
              </select>
            </label>

            <label className="block text-xs font-medium text-gray-600 dark:text-gray-300">
              项目
              <input value={project} onChange={e => setProject(e.target.value)} className="mt-1.5 w-full rounded-md border border-gray-200 bg-white px-3 py-2 text-sm dark:border-gray-700 dark:bg-gray-800" placeholder="可选" />
            </label>

            <label className="block text-xs font-medium text-gray-600 dark:text-gray-300">
              标签
              <input value={tags} onChange={e => setTags(e.target.value)} className="mt-1.5 w-full rounded-md border border-gray-200 bg-white px-3 py-2 text-sm dark:border-gray-700 dark:bg-gray-800" placeholder="Rust, Tauri, Windows" />
              <span className="mt-1 block text-[10px] text-gray-400">使用逗号分隔</span>
            </label>

            <label className="block text-xs font-medium text-gray-600 dark:text-gray-300">
              摘要
              <textarea value={summary} onChange={e => setSummary(e.target.value)} rows={5} className="mt-1.5 w-full resize-none rounded-md border border-gray-200 bg-white px-3 py-2 text-sm dark:border-gray-700 dark:bg-gray-800" placeholder="可选，用于列表快速浏览" />
            </label>
          </div>

          <div className="flex min-h-0 flex-col">
            <div className="flex items-center justify-between border-b border-gray-100 px-4 py-2 dark:border-gray-800">
              <div className="flex rounded-md bg-gray-100 p-0.5 dark:bg-gray-800">
                <button type="button" onClick={() => setPreview(false)} className={`rounded px-3 py-1 text-xs ${!preview ? "bg-white shadow-sm dark:bg-gray-700" : "text-gray-500"}`}>Markdown</button>
                <button type="button" onClick={() => setPreview(true)} className={`flex items-center gap-1 rounded px-3 py-1 text-xs ${preview ? "bg-white shadow-sm dark:bg-gray-700" : "text-gray-500"}`}><Eye className="h-3 w-3" />预览</button>
              </div>
              <span className="text-[10px] text-gray-400">{content.length} 字符</span>
            </div>

            {preview ? (
              <div className="flex-1 overflow-y-auto p-6">
                <pre className="whitespace-pre-wrap font-sans text-sm leading-7 text-gray-700 dark:text-gray-200">{content || "暂无内容"}</pre>
              </div>
            ) : (
              <textarea value={content} onChange={e => setContent(e.target.value)} className="flex-1 resize-none border-0 bg-transparent p-5 font-mono text-sm leading-6 outline-none" placeholder="# 知识内容\n\n支持 Markdown，可粘贴代码、命令、方案和结论。" spellCheck={false} />
            )}
          </div>
        </div>

        <div className="flex items-center justify-between border-t border-gray-200 px-5 py-3 dark:border-gray-700">
          <div className="text-xs text-red-500">{error}</div>
          <div className="flex gap-2">
            <button type="button" onClick={onClose} className="rounded-md border border-gray-200 px-4 py-2 text-sm text-gray-600 hover:bg-gray-50 dark:border-gray-700 dark:text-gray-300 dark:hover:bg-gray-800">取消</button>
            <button type="button" onClick={() => void save()} disabled={saving} className="inline-flex items-center gap-1.5 rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-700 disabled:opacity-50">
              {saving ? <Loader2 className="h-4 w-4 animate-spin" /> : <Save className="h-4 w-4" />}
              {saving ? "保存中..." : "保存知识"}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
