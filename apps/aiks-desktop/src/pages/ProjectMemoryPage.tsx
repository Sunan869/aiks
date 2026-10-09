import { useEffect, useState } from "react";
import { getApi } from "../api/client";
import type { ProjectMemorySnapshot, ProjectOverview } from "../api/types";

interface Props {
  onOpenKnowledge: (knowledgeId: string) => void;
}

function todayDate(): string { return new Date().toISOString().slice(0, 10); }
function weekAgo(): string { return new Date(Date.now() - 7 * 86400000).toISOString().slice(0, 10); }

export default function ProjectMemoryPage({ onOpenKnowledge }: Props) {
  const [projects, setProjects] = useState<ProjectOverview[]>([]);
  const [selected, setSelected] = useState("");
  const [memory, setMemory] = useState<ProjectMemorySnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [from, setFrom] = useState(weekAgo());
  const [through, setThrough] = useState(todayDate());
  const [tokenBudget, setTokenBudget] = useState(1500);
  const [preview, setPreview] = useState("");
  const [previewKind, setPreviewKind] = useState("");

  useEffect(() => {
    let active = true;
    void getApi().getProjectsMemory().then(items => {
      if (!active) return;
      setProjects(items);
      if (items.length) setSelected(current => current || items[0].id);
    }).catch(reason => { if (active) setError(String(reason)); });
    return () => { active = false; };
  }, []);

  useEffect(() => {
    if (!selected) return;
    let active = true;
    setMemory(null);
    setPreview("");
    setPreviewKind("");
    void getApi().getProjectMemory(selected).then(snapshot => {
      if (active) setMemory(snapshot);
    }).catch(reason => { if (active) setError(String(reason)); });
    return () => { active = false; };
  }, [selected]);

  const createPreview = async (kind: "review" | "agent") => {
    if (!selected || busy) return;
    setBusy(true);
    setError(null);
    try {
      const text = kind === "review"
        ? await getApi().createProjectReview(selected, from, through)
        : await getApi().createAgentContextPack(selected, tokenBudget);
      setPreview(text);
      setPreviewKind(kind);
    } catch (reason) {
      setError("生成失败：" + String(reason));
    } finally {
      setBusy(false);
    }
  };

  const copyPreview = async () => {
    try {
      await navigator.clipboard.writeText(preview);
    } catch (reason) {
      setError("复制失败，请手动选中预览文本：" + String(reason));
    }
  };

  return (
    <div className="h-full overflow-y-auto p-6">
      <div className="mb-5">
        <h1 className="text-xl font-semibold text-gray-900 dark:text-gray-100">项目长期记忆</h1>
        <p className="mt-1 text-xs text-gray-500">按真实项目路径归并跨工具 Session；只有项目名、没有路径的 Session 不会猜测合并。</p>
      </div>
      <div className="mb-5 flex flex-wrap items-center gap-3">
        <label htmlFor="project-memory-select" className="text-sm text-gray-600 dark:text-gray-300">项目</label>
        <select id="project-memory-select" value={selected} onChange={event => setSelected(event.target.value)}
          className="min-w-0 max-w-lg flex-1 rounded border border-gray-200 bg-white p-2 text-sm dark:border-gray-700 dark:bg-gray-800">
          {projects.length === 0 && <option value="">暂无已识别项目</option>}
          {projects.map(project => (
            <option key={project.id} value={project.id}>{project.title} · {project.session_count} Session</option>
          ))}
        </select>
      </div>
      {error && <p className="mb-4 rounded border border-red-200 bg-red-50 p-3 text-xs text-red-600" role="alert">{error}</p>}
      {memory && (
        <>
          <div className="mb-5 grid gap-3 sm:grid-cols-3">
            <div className="rounded border border-gray-200 p-3 dark:border-gray-700"><div className="text-xs text-gray-500">Session</div><div className="text-xl font-semibold">{memory.project.session_count}</div></div>
            <div className="rounded border border-gray-200 p-3 dark:border-gray-700"><div className="text-xs text-gray-500">知识条目</div><div className="text-xl font-semibold">{memory.project.knowledge_count}</div></div>
            <div className="rounded border border-gray-200 p-3 dark:border-gray-700"><div className="text-xs text-gray-500">归属方式</div><div className="mt-1 text-sm">{memory.project.verified_path ? "项目路径校验" : "单会话隔离（待人工绑定）"}</div></div>
          </div>
          <section className="mb-6 rounded border border-gray-200 p-4 dark:border-gray-700">
            <h2 className="mb-2 text-sm font-semibold">项目知识时间线</h2>
            {memory.truncated && <p className="mb-3 text-xs text-amber-600">当前仅展示最多 200 条；报告可能不完整。</p>}
            {memory.entries.length === 0 && <p className="text-xs text-gray-500">该项目尚无可展示的已提炼知识。</p>}
            <div className="space-y-3">
              {memory.entries.map(entry => (
                <div key={entry.knowledge_id} className="border-t border-gray-100 pt-3 dark:border-gray-800">
                  <button onClick={() => onOpenKnowledge(entry.knowledge_id)} className="text-left text-sm font-medium text-blue-600 hover:underline">{entry.title}</button>
                  <p className="mt-1 text-xs text-gray-500">{entry.category} · {entry.source} · {entry.updated_at.slice(0, 10)} · Session {entry.session_external_id}</p>
                  {entry.feedback_status === "incorrect" || entry.feedback_status === "outdated" ? (
                    <p className="mt-1 text-xs text-amber-600">人工质量标记：{entry.feedback_status === "incorrect" ? "有误" : "已过时"}</p>
                  ) : null}
                  <p className="mt-1 whitespace-pre-wrap text-xs text-gray-600 dark:text-gray-300">{entry.summary}</p>
                </div>
              ))}
            </div>
          </section>
          <section className="rounded border border-gray-200 p-4 dark:border-gray-700">
            <h2 className="mb-2 text-sm font-semibold">工作回顾与 Agent 知识包</h2>
            <p className="mb-3 text-xs text-gray-500">仅生成本地可编辑 Markdown 预览，不自动发送或修改项目中的 AGENTS.md / CLAUDE.md。</p>
            <div className="flex flex-wrap items-center gap-2 text-xs">
              <label>开始 <input type="date" value={from} onChange={event => setFrom(event.target.value)} className="rounded border p-1 dark:bg-gray-800" /></label>
              <label>结束 <input type="date" value={through} onChange={event => setThrough(event.target.value)} className="rounded border p-1 dark:bg-gray-800" /></label>
              <button type="button" onClick={() => void createPreview("review")} disabled={busy} className="rounded bg-blue-600 px-3 py-1.5 text-white disabled:opacity-50">生成工作回顾</button>
              <label>Token 预算估算 <input type="number" min={256} max={8192} value={tokenBudget} onChange={event => setTokenBudget(Number(event.target.value) || 256)} className="w-20 rounded border p-1 dark:bg-gray-800" /></label>
              <button type="button" onClick={() => void createPreview("agent")} disabled={busy} className="rounded bg-indigo-600 px-3 py-1.5 text-white disabled:opacity-50">生成 Agent 上下文</button>
            </div>
            {previewKind && (
              <div className="mt-3">
                <div className="mb-2 flex items-center justify-between text-xs">
                  <span>{previewKind === "review" ? "工作回顾预览（可修改）" : "Agent 上下文预览（可修改）"}</span>
                  <button type="button" onClick={() => void copyPreview()} className="rounded border px-2 py-1">复制 Markdown</button>
                </div>
                <textarea value={preview} onChange={event => setPreview(event.target.value)} rows={14}
                  className="w-full rounded border border-gray-200 bg-white p-3 font-mono text-xs dark:border-gray-700 dark:bg-gray-900" />
              </div>
            )}
          </section>
        </>
      )}
    </div>
  );
}
