import { useEffect, useRef, useState } from "react";
import { getApi } from "../api/client";
import { AGENT_EXPORT_FORMATS, projectExportFileName, renderAgentSuggestion, type AgentExportFormat } from "../agent-export";
import type { ProjectEvolutionSnapshot, ProjectMemoryEntry, ProjectMemorySnapshot, ProjectOverview } from "../api/types";
import { PROJECT_MEMORY_TOPICS, countProjectMemoryTopics, groupProjectMemoryUpdates, recordedUpdateDay, selectProjectMemoryEntries, type ProjectMemoryTopicFilter } from "../project-memory-view";
import { summarizeProjectMemoryQuality } from "../project-memory-quality";

interface Props {
  onOpenKnowledge: (knowledgeId: string) => void;
  onOpenSession: (sessionId: number) => void;
}

/** Every UI fact links to the canonical Knowledge and source Session. */
function ProjectMemoryEvidenceCard({ item, onOpenKnowledge, onOpenSession }: {
  item: ProjectMemoryEntry;
  onOpenKnowledge: Props["onOpenKnowledge"];
  onOpenSession: Props["onOpenSession"];
}) {
  return (
    <article className="rounded border p-3 dark:border-gray-700">
      <div className="flex flex-wrap justify-between gap-2">
        <strong>{item.title}</strong>
        <span className="text-xs text-gray-500">{recordedUpdateDay(item.updated_at) || "更新时间未知"} · {item.category} · {item.source}</span>
      </div>
      <p className="my-2 whitespace-pre-wrap text-xs text-gray-600 dark:text-gray-400">{item.summary}</p>
      {item.feedback_status && <span className="text-xs text-amber-600">最近用户反馈：{item.feedback_status}</span>}
      <div className="mt-2 flex gap-3 text-xs">
        <button type="button" className="text-blue-600" onClick={() => onOpenKnowledge(item.knowledge_id)}>知识详情</button>
        <button type="button" className="text-blue-600" onClick={() => onOpenSession(item.session_id)}>原始 Session</button>
      </div>
    </article>
  );
}

export default function ProjectMemoryPage({ onOpenKnowledge, onOpenSession }: Props) {
  const [projects, setProjects] = useState<ProjectOverview[]>([]);
  const [selected, setSelected] = useState("");
  const [detail, setDetail] = useState<ProjectMemorySnapshot | null>(null);
  const [evolution, setEvolution] = useState<ProjectEvolutionSnapshot | null>(null);
  const [evolutionError, setEvolutionError] = useState("");
  const [relationBusy, setRelationBusy] = useState<string | null>(null);
  const [error, setError] = useState("");
  const [working, setWorking] = useState(false);
  const [from, setFrom] = useState(() => new Date(Date.now() - 6 * 86400000).toISOString().slice(0,10));
  const [through, setThrough] = useState(() => new Date().toISOString().slice(0,10));
  const [tokenBudget, setTokenBudget] = useState(2000);
  const [preview, setPreview] = useState("");
  const [previewType, setPreviewType] = useState<"review" | "agent" | "">("");
  const generation = useRef(0);
  const projectGeneration = useRef(0);
  const [notice, setNotice] = useState("");
  const [onlyFlagged, setOnlyFlagged] = useState(false);
  const [sourceFilter, setSourceFilter] = useState("");
  const [agentFormat, setAgentFormat] = useState<AgentExportFormat>("markdown");
  const [topicFilter, setTopicFilter] = useState<ProjectMemoryTopicFilter>("all");
  const [viewMode, setViewMode] = useState<"list" | "timeline">("list");

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
    if (!selected) { setDetail(null); setEvolution(null); return; }
    let mounted = true;
    setError("");
    generation.current += 1;
    projectGeneration.current += 1;
    setPreview("");
    setPreviewType("");
    setNotice("");
    setWorking(false);
    setDetail(null);
    setEvolution(null);
    setEvolutionError("");
    setRelationBusy(null);
    setOnlyFlagged(false);
    setSourceFilter("");
    setAgentFormat("markdown");
    setTopicFilter("all");
    setViewMode("list");
    const api = getApi();
    void api.getProjectMemory(selected).then(snapshot => {
      if (mounted) setDetail(snapshot);
    }).catch(reason => { if (mounted) setError(String(reason)); });
    void api.getProjectEvolution(selected).then(snapshot => {
      if (mounted) setEvolution(snapshot);
    }).catch(reason => {
      if (mounted) setEvolutionError(String(reason));
    });
    return () => { mounted = false; };
  }, [selected]);

  // Changing any export parameter invalidates old, potentially edited Markdown.
  // It also invalidates responses from a request started under older settings.
  const invalidatePreview = () => {
    generation.current += 1;
    setPreview("");
    setPreviewType("");
    setNotice("");
    setWorking(false);
  };

  // A human decision updates the existing append-only relation audit; it
  // never edits either knowledge item. Re-fetch canonical projection after
  // completion to avoid showing an obsolete suggested relationship.
  const reviewEvolution = async (relationId: string, decision: "confirmed" | "rejected") => {
    if (!selected || relationBusy !== null) return;
    const prompt = decision === "confirmed"
      ? "确认将这两条知识记录建立为已审核的演进关系？请先核对双方原文及关系证据。"
      : "拒绝此演进关系建议？原始知识和 Session 均会保留。";
    if (!window.confirm(prompt)) return;
    const requestProject = selected;
    const requestGeneration = projectGeneration.current;
    setRelationBusy(relationId);
    setEvolutionError("");
    try {
      await getApi().reviewKnowledgeRelation(relationId, decision);
      if (requestGeneration !== projectGeneration.current) return;
      const snapshot = await getApi().getProjectEvolution(requestProject);
      if (requestGeneration !== projectGeneration.current) return;
      setEvolution(snapshot);
      setNotice(decision === "confirmed" ? "已保存人工确认的关系审核。" : "已拒绝关系建议，原始知识不受影响。");
    } catch (reason) {
      if (requestGeneration === projectGeneration.current) {
        setEvolutionError("关系审核未完成：" + String(reason));
      }
    } finally {
      if (requestGeneration === projectGeneration.current) setRelationBusy(null);
    }
  };

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
        ? await getApi().createProjectReview(selected, from, through, sourceFilter || undefined)
        : await getApi().createAgentContextPack(selected, tokenBudget, sourceFilter || undefined);
      if (requestId !== generation.current) return;
      setPreviewType(kind);
      setPreview(kind === "agent" ? renderAgentSuggestion(output, agentFormat) : output);
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
    const filename = projectExportFileName(
      detail?.project.title,
      previewType === "review" ? "review" : "agent",
      agentFormat,
    );
    const blob = new Blob([preview], { type: "text/markdown;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    try {
      const anchor = document.createElement("a");
      anchor.href = url;
      anchor.download = filename;
      anchor.click();
      setNotice("已请求保存审核后的 Markdown 文件，未修改任何项目指令文件。");
    } finally {
      window.setTimeout(() => URL.revokeObjectURL(url), 0);
    }
  };

  const evidence = detail ? selectProjectMemoryEntries(detail.entries, {
    topic: topicFilter,
    feedbackOnly: onlyFlagged,
  }) : [];
  const topicCounts = detail ? countProjectMemoryTopics(detail.entries) : null;
  const timeline = viewMode === "timeline" ? groupProjectMemoryUpdates(evidence) : [];
  const evidenceCard = (item: ProjectMemoryEntry) => (
    <ProjectMemoryEvidenceCard
      key={item.knowledge_id}
      item={item}
      onOpenKnowledge={onOpenKnowledge}
      onOpenSession={onOpenSession}
    />
  );

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
            {(() => {
              const quality = summarizeProjectMemoryQuality(detail.entries);
              const flagged = quality.incorrect + quality.outdated + quality.needsDetail + quality.duplicate;
              return flagged > 0 ? (
                <p role="status" className="mt-3 rounded border border-amber-300 p-2 text-xs text-amber-700">
                  当前加载的 {detail.entries.length} 条知识中，用户质量标记为：错误 {quality.incorrect}、过时 {quality.outdated}、需补充 {quality.needsDetail}、可能重复 {quality.duplicate}。{detail.truncated && "列表已截断，统计不覆盖全部项目知识。"}请逐条核对来源；该统计不代表自动识别事实矛盾。
                </p>
              ) : null;
            })()}
            <div className="mt-4 space-y-2">
              <h3 className="text-sm font-medium">项目记忆主题概览（当前已加载的知识）</h3>
              <p className="text-xs text-gray-500">
                按既有知识类别整理，不从对话猜测已完成事项、待办或事实矛盾。统计仅覆盖当前有界样本。
              </p>
              <div role="group" aria-label="项目记忆主题" className="flex flex-wrap gap-2">
                <button type="button" aria-pressed={topicFilter === "all"}
                  className={`rounded border px-2 py-1 text-xs ${topicFilter === "all" ? "border-blue-500 bg-blue-50 text-blue-900" : "dark:border-gray-700"}`}
                  onClick={() => setTopicFilter("all")}>全部主题 · {detail.entries.length}</button>
                {PROJECT_MEMORY_TOPICS.map(topic => (
                  <button type="button" key={topic.id} aria-pressed={topicFilter === topic.id}
                    className={`rounded border px-2 py-1 text-xs ${topicFilter === topic.id ? "border-blue-500 bg-blue-50 text-blue-900" : "dark:border-gray-700"}`}
                    onClick={() => setTopicFilter(topic.id)}>
                    {topic.title} · {topicCounts?.[topic.id] ?? 0}
                  </button>
                ))}
              </div>
            </div>
            <div className="mt-4 flex flex-wrap items-center justify-between gap-2 text-xs">
              <label className="flex items-center gap-2">
                <input type="checkbox" checked={onlyFlagged} onChange={event => setOnlyFlagged(event.target.checked)} />
                仅查看需要人工复核的知识（基于用户反馈）
              </label>
              <button type="button" aria-pressed={viewMode === "timeline"}
                className="rounded border px-3 py-2 dark:border-gray-700"
                onClick={() => setViewMode(current => current === "timeline" ? "list" : "timeline")}>
                {viewMode === "timeline" ? "切换为列表" : "按知识更新时间线"}
              </button>
            </div>
            {viewMode === "timeline" && (
              <p className="mt-2 text-xs text-amber-700">
                时间线使用知识最后更新时间，并非问题发生或决策生效时间；请点击原始 Session 核实。
              </p>
            )}
            <div className="mt-4 space-y-3">
              {viewMode === "timeline" ? timeline.map(group => (
                <section key={group.day} aria-label={`知识更新 ${group.day}`} className="space-y-2">
                  <h3 className="font-medium text-gray-600 dark:text-gray-300">知识更新 · {group.day}</h3>
                  {group.entries.map(evidenceCard)}
                </section>
              )) : evidence.map(evidenceCard)}
              {evidence.length === 0 && <p className="text-gray-500">当前筛选范围内暂无知识证据。</p>}
              {detail.truncated && <p className="text-amber-600">当前仅展示有界样本，不代表全部项目历史；主题统计和时间线均可能不完整。</p>}
            </div>
          </section>
          <section className="rounded border p-4 dark:border-gray-700">
            <h2 className="font-semibold">知识演进与人工审核关系</h2>
            <p className="mt-2 text-xs text-gray-500">
              仅显示带来源证据、由用户确认或尚待确认的「修正 / 替代 / 解决」关系。
              关系时间表示审核记录更新时间，不代表方案生效时间；旧知识及 Session 均保留。
            </p>
            {evolutionError && <p role="alert" className="mt-2 text-xs text-red-600">
              关系读取失败：{evolutionError}
            </p>}
            {!evolution && !evolutionError && <p className="mt-2 text-xs text-gray-500">正在读取关系…</p>}
            {evolution && evolution.links.length === 0 && (
              <p className="mt-2 text-xs text-gray-500">
                当前项目没有可展示的人工确认或待确认演进关系；这并不证明不存在知识矛盾。
              </p>
            )}
            {evolution && evolution.links.length > 0 && (
              <div className="mt-3 space-y-2">
                {evolution.links.map(link => {
                  const relationLabel = {
                    corrects: "修正",
                    supersedes: "替代",
                    resolved_by: "通过…解决",
                  }[link.relation_type];
                  return (
                    <article key={link.id} className="rounded border p-3 text-xs dark:border-gray-700">
                      <div className="flex flex-wrap items-center gap-2">
                        <span className={link.status === "confirmed" ?
                          "rounded bg-green-50 px-2 py-1 text-green-800" :
                          "rounded bg-amber-50 px-2 py-1 text-amber-800"}>
                          {link.status === "confirmed" ? "用户已确认关系" : "待人工审核，不能视为生效结论"}
                        </span>
                        <span className="text-gray-500">{relationLabel} · 关系更新 {link.updated_at.slice(0, 10)}</span>
                      </div>
                      <div className="mt-2 flex flex-wrap items-center gap-2">
                        <button type="button" className="text-blue-600 hover:underline"
                          onClick={() => onOpenKnowledge(link.source_id)}>
                          {link.source_title}
                        </button>
                        <span className="text-gray-400">→ {relationLabel} →</span>
                        <button type="button" className="text-blue-600 hover:underline"
                          onClick={() => onOpenKnowledge(link.target_id)}>
                          {link.target_title}
                        </button>
                      </div>
                      <p className="mt-2 whitespace-pre-wrap break-words text-gray-600 dark:text-gray-300">
                        审核证据：{link.evidence}
                      </p>
                      {link.status === "suggested" && (
                        <div className="mt-3 flex flex-wrap gap-2">
                          <button type="button" disabled={relationBusy !== null}
                            className="rounded border border-green-400 px-2 py-1 text-green-700 disabled:opacity-40"
                            onClick={() => void reviewEvolution(link.id, "confirmed")}>
                            {relationBusy === link.id ? "审核提交中…" : "核对原文后确认关系"}
                          </button>
                          <button type="button" disabled={relationBusy !== null}
                            className="rounded border border-gray-300 px-2 py-1 text-gray-700 disabled:opacity-40"
                            onClick={() => void reviewEvolution(link.id, "rejected")}>
                            拒绝关系建议
                          </button>
                        </div>
                      )}
                    </article>
                  );
                })}
                {evolution.truncated && (
                  <p className="text-amber-600">仅展示最近 100 条关系，不代表项目全部历史。</p>
                )}
              </div>
            )}
          </section>
          <section className="rounded border p-4 dark:border-gray-700">
            <h2 className="font-semibold">工作回顾与 Agent 上下文</h2>
            <p className="mt-2 text-xs text-gray-500">先生成可编辑预览，再复制使用。不会自动写入项目文件或外部服务。</p>
            <label htmlFor="project-memory-source" className="my-3 flex items-center gap-2 text-xs">
              来源范围
              <select id="project-memory-source" value={sourceFilter}
                className="rounded border bg-transparent p-2 dark:border-gray-700"
                onChange={event => {
                  invalidatePreview();
                  setSourceFilter(event.target.value);
                }}>
                <option value="">全部来源</option>
                {detail.project.sources.map(source => <option key={source} value={source}>{source}</option>)}
              </select>
            </label>
            <div className="my-3 flex flex-wrap items-center gap-2 text-xs">
              <label>开始日期 <input aria-label="回顾开始日期" type="date" value={from} onChange={e => { invalidatePreview(); setFrom(e.target.value); }} className="rounded border bg-transparent p-1 dark:border-gray-700" /></label>
              <label>结束日期 <input aria-label="回顾结束日期" type="date" value={through} onChange={e => { invalidatePreview(); setThrough(e.target.value); }} className="rounded border bg-transparent p-1 dark:border-gray-700" /></label>
              <button disabled={working || !from || !through || from > through} onClick={() => void generate("review")} className="rounded bg-blue-600 px-3 py-2 text-white disabled:opacity-40">生成项目回顾</button>
            </div>
            <div className="mb-3 space-y-2 text-xs">
              <label className="flex items-center gap-2">
                Agent 导出格式
                <select aria-label="Agent 导出格式" value={agentFormat}
                  className="rounded border bg-transparent p-2 dark:border-gray-700"
                  onChange={event => {
                    invalidatePreview();
                    setAgentFormat(event.target.value as AgentExportFormat);
                  }}>
                  {AGENT_EXPORT_FORMATS.map(format => (
                    <option key={format.value} value={format.value}>{format.label}</option>
                  ))}
                </select>
              </label>
              <p className="text-gray-500">
                AGENTS.md / CLAUDE.md 仅生成可审核的引用建议片段，不自动读取、覆盖或合并已有文件。
                切换来源、格式或预算会清空旧预览，避免错误导出。
              </p>
            </div>
            <div className="mb-3 flex flex-wrap items-center gap-2 text-xs">
              <label>上下文 Token 预算（估算）
                <input aria-label="Token 预算" type="number" min={256} max={8192} value={tokenBudget}
                  onChange={e => { invalidatePreview(); setTokenBudget(Math.max(256, Math.min(8192, Number(e.target.value) || 256))); }} className="ml-2 w-24 rounded border bg-transparent p-1 dark:border-gray-700" />
              </label>
              <button disabled={working} onClick={() => void generate("agent")} className="rounded bg-indigo-600 px-3 py-2 text-white disabled:opacity-40">生成 Agent 上下文</button>
            </div>
            {previewType && (
              <div className="space-y-2">
                <p className="text-xs text-gray-500">{previewType === "review" ? "回顾 Markdown" : agentFormat === "markdown" ? "Agent Markdown" : `${agentFormat === "agents" ? "AGENTS.md" : "CLAUDE.md"} 建议片段`} · 可编辑预览</p>
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
