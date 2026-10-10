import { useEffect, useRef, useState } from "react";
import { getApi } from "../api/client";
import type { ProjectMemorySnapshot, ProjectOverview } from "../api/types";

interface Props {
  onOpenKnowledge: (knowledgeId: string) => void;
  onOpenSession: (sessionId: number) => void;
}

export default function ProjectMemoryPage({ onOpenKnowledge, onOpenSession }: Props) {
  const [projects, setProjects] = useState<ProjectOverview[]>([]);
  const [selected, setSelected] = useState("");
  const [detail, setDetail] = useState<ProjectMemorySnapshot | null>(null);
  const [error, setError] = useState("");
  const [working, setWorking] = useState(false);
  const [from, setFrom] = useState(() => new Date(Date.now() - 6 * 86400000).toISOString().slice(0,10));
  const [through, setThrough] = useState(() => new Date().toISOString().slice(0,10));
  const [tokenBudget, setTokenBudget] = useState(2000);
  const [preview, setPreview] = useState("");
  const [previewType, setPreviewType] = useState<"review" | "agent" | "">("");
  const generation = useRef(0);
  const [notice, setNotice] = useState("");

  useEffect(() => {
    let mounted = true;
    getApi().getProjectMemories().then(items => {
      if (mounted) {
        setProjects(items);
        setSelected(current => current || items[0]?.id || "");
      }
    }).catch(reason => { if (mounted) setError(String(reason)); });
    return () => { mounted = false; };
  }, []);

  useEffect(() => {
    if (!selected) { setDetail(null); return; }
    let mounted = true;
    setError("");
    generation.current += 1;
    setPreview("");
    setPreviewType("");
    setNotice("");
    setWorking(false);
    setDetail(null);
    getApi().getProjectMemory(selected).then(snapshot => {
      if (mounted) setDetail(snapshot);
    }).catch(reason => { if (mounted) setError(String(reason)); });
    return () => { mounted = false; };
  }, [selected]);

  const generate = async (kind: "review" | "agent") => {
    if (!selected || working) return;
    if (kind === "review" && (!from || !through || from > through)) {
      setError("请选择有效的日期范围，开始日期不能晚于结束日期。");
      return;
    }
    const requestId = ++generation.current;
    setError("");
    setNotice("");
    setWorking(true);
    try {
      const output = kind === "review"
        ? await getApi().createProjectReview(selected, from, through)
        : await getApi().createAgentContextPack(selected, tokenBudget);
      if (requestId !== generation.current) return;
      setPreviewType(kind);
      setPreview(output);
    } catch (reason) {
      if (requestId === generation.current) setError(String(reason));
    } finally {
      if (requestId === generation.current) setWorking(false);
    }
  };

  const copyPreview = async () => {
    try { await navigator.clipboard.writeText(preview); setNotice("已复制当前审核后的 Markdown。"); }
    catch (reason) { setError("复制失败：" + String(reason)); }
  };

  const downloadPreview = () => {
    if (!preview.trim() || !previewType) return;
    const safeProject = (detail?.project.title || "project")
      .replace(/[\\\\/:*?"<>|\\u0000-\\u001f]/g, "_").trim().slice(0, 60) || "project";
    const suffix = previewType === "review" ? "review" : "agent-context";
    const blob = new Blob([preview], { type: "text/markdown;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    try {
      const anchor = document.createElement("a");
      anchor.href = url;
      anchor.download = `AIKS-${safeProject}-${suffix}.md`;
      anchor.click();
      setNotice("已请求保存审核后的 Markdown 文件，未修改任何项目指令文件。");
    } finally {
      window.setTimeout(() => URL.revokeObjectURL(url), 0);
    }
  };

  return (
    <div className="space-y-5 p-6 text-sm">
      <header>
        <h1 className="text-xl font-semibold">项目长期记忆</h1>
        <p className="mt-2 text-xs text-gray-500">按已记录的项目路径归集不同 AI 工具的 Session。无法确认路径的会话不会仅凭同名自动合并。</p>
      </header>
      {notice && <p role="status" className="rounded bg-green-50 p-3 text-green-800">{notice}</p>}
      {error && <p role="alert" className="rounded bg-red-50 p-3 text-red-700">{error}</p>}
      <div className="flex items-center gap-3">
        <label htmlFor="project-memory-select">项目</label>
        <select id="project-memory-select" className="min-w-0 flex-1 rounded border bg-transparent p-2 dark:border-gray-700"
          value={selected} onChange={event => setSelected(event.target.value)}>
          {projects.map(item => <option key={item.id} value={item.id}>{item.title} · {item.session_count} 会话</option>)}
        </select>
      </div>
      {projects.length === 0 && <p className="text-gray-500">目前没有可用的项目 Session，先同步本地工作记录。</p>}
      {detail && (
        <>
          <section className="rounded border p-4 dark:border-gray-700">
            <h2 className="font-semibold">{detail.project.title}</h2>
            <p className="mt-2 text-xs text-gray-500">
              {detail.project.session_count} 条会话 · {detail.project.knowledge_count} 条知识 · 来源：{detail.project.sources.join("、")}
              {!detail.project.verified_path && " · 路径未确认，独立归档"}
            </p>
            <div className="mt-4 space-y-3">
              {detail.entries.map(item => (
                <article key={item.knowledge_id} className="rounded border p-3 dark:border-gray-700">
                  <div className="flex flex-wrap justify-between gap-2">
                    <strong>{item.title}</strong>
                    <span className="text-xs text-gray-500">{item.updated_at.slice(0,10)} · {item.category}</span>
                  </div>
                  <p className="my-2 whitespace-pre-wrap text-xs text-gray-600 dark:text-gray-400">{item.summary}</p>
                  {item.feedback_status && <span className="text-xs text-amber-600">最近用户反馈：{item.feedback_status}</span>}
                  <div className="mt-2 flex gap-3 text-xs">
                    <button className="text-blue-600" onClick={() => onOpenKnowledge(item.knowledge_id)}>知识详情</button>
                    <button className="text-blue-600" onClick={() => onOpenSession(item.session_id)}>原始 Session</button>
                  </div>
                </article>
              ))}
              {detail.entries.length === 0 && <p className="text-gray-500">暂无可追溯的结构化知识。</p>}
              {detail.truncated && <p className="text-amber-600">当前仅展示有界样本，不代表全部项目历史。</p>}
            </div>
          </section>
          <section className="rounded border p-4 dark:border-gray-700">
            <h2 className="font-semibold">工作回顾与 Agent 上下文</h2>
            <p className="mt-2 text-xs text-gray-500">先生成可编辑预览，再复制使用。不会自动写入项目文件或外部服务。</p>
            <div className="my-3 flex flex-wrap items-center gap-2 text-xs">
              <label>开始日期 <input aria-label="回顾开始日期" type="date" value={from} onChange={e => setFrom(e.target.value)} className="rounded border bg-transparent p-1 dark:border-gray-700" /></label>
              <label>结束日期 <input aria-label="回顾结束日期" type="date" value={through} onChange={e => setThrough(e.target.value)} className="rounded border bg-transparent p-1 dark:border-gray-700" /></label>
              <button disabled={working || !from || !through || from > through} onClick={() => void generate("review")} className="rounded bg-blue-600 px-3 py-2 text-white disabled:opacity-40">生成项目回顾</button>
            </div>
            <div className="mb-3 flex flex-wrap items-center gap-2 text-xs">
              <label>上下文 Token 预算（估算）
                <input aria-label="Token 预算" type="number" min={256} max={8192} value={tokenBudget}
                  onChange={e => setTokenBudget(Math.max(256, Math.min(8192, Number(e.target.value) || 256)))} className="ml-2 w-24 rounded border bg-transparent p-1 dark:border-gray-700" />
              </label>
              <button disabled={working} onClick={() => void generate("agent")} className="rounded bg-indigo-600 px-3 py-2 text-white disabled:opacity-40">生成 Agent 上下文</button>
            </div>
            {previewType && (
              <div className="space-y-2">
                <p className="text-xs text-gray-500">{previewType === "review" ? "回顾 Markdown" : "Agent Markdown"} · 可编辑预览</p>
                <textarea aria-label="导出内容预览" value={preview} onChange={e => setPreview(e.target.value)} rows={15}
                  className="w-full rounded border bg-transparent p-3 font-mono text-xs dark:border-gray-700" />
                <div className="flex flex-wrap gap-2">
                  <button disabled={!preview.trim()} onClick={() => void copyPreview()} className="rounded border px-3 py-2 text-xs disabled:opacity-40 dark:border-gray-700">复制审核后的 Markdown</button>
                  <button disabled={!preview.trim()} onClick={downloadPreview} className="rounded border px-3 py-2 text-xs disabled:opacity-40 dark:border-gray-700">保存审核后的 Markdown</button>
                </div>
              </div>
            )}
          </section>
        </>
      )}
    </div>
  );
}
