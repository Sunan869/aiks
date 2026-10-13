import { UiInput, UiTextarea } from "../components/ui/fields";
import { UiButton } from "../components/ui/fields";
import { CalendarDateField } from "../components/ui/CalendarDateField";
import { SearchSelect } from "../components/ui/SearchSelect";
import { UiSelect } from "../components/ui/fields";
import { useEffect, useRef, useState } from "react";
import { getApi } from "../api/client";
import { AGENT_EXPORT_FORMATS, projectExportFileName, renderAgentSuggestion, type AgentExportFormat } from "../agent-export";
import type { ProjectEvolutionSnapshot, ProjectMemoryEntry, ProjectMemorySnapshot, ProjectOverview, ProjectReportDraft, ProjectSessionBinding, SessionItem } from "../api/types";
import { PROJECT_MEMORY_TOPICS, countProjectMemoryTopics, groupProjectMemoryUpdates, recordedUpdateDay, selectProjectMemoryEntries, type ProjectMemoryTopicFilter } from "../project-memory-view";
import { summarizeProjectMemoryQuality } from "../project-memory-quality";
import { buildProjectReviewQueue, type ProjectReviewSignal } from "../project-review-queue";
import { reportDateRange } from "../project-report-dates";

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
    <article className="memory-evidence">
      <div className="flex flex-wrap justify-between gap-2">
        <strong>{item.title}</strong>
        <span className="text-xs text-gray-500">{recordedUpdateDay(item.updated_at) || "更新时间未知"} · {item.category} · {item.source}</span>
      </div>
      <p className="my-2 whitespace-pre-wrap text-xs text-gray-600 dark:text-gray-400">{item.summary}</p>
      {item.feedback_status && <span className="text-xs text-amber-600">最近用户反馈：{item.feedback_status}</span>}
      <div className="mt-2 flex gap-3 text-xs">
        <UiButton variant="outline" size="sm" type="button" className="text-blue-600" onClick={() => onOpenKnowledge(item.knowledge_id)}>知识详情</UiButton>
        <UiButton variant="outline" size="sm" type="button" className="text-blue-600" onClick={() => onOpenSession(item.session_id)}>原始 Session</UiButton>
      </div>
    </article>
  );
}

export default function ProjectMemoryPage({ onOpenKnowledge, onOpenSession }: Props) {
  const [projects, setProjects] = useState<ProjectOverview[]>([]);
  const [selected, setSelected] = useState("");
  const [detail, setDetail] = useState<ProjectMemorySnapshot | null>(null);
  const [evolution, setEvolution] = useState<ProjectEvolutionSnapshot | null>(null);
  const [evolutionProjectId, setEvolutionProjectId] = useState<string | null>(null);
  const [reviewQueueFilter, setReviewQueueFilter] = useState<"all" | ProjectReviewSignal>("all");
  const [evolutionError, setEvolutionError] = useState("");
  const [relationBusy, setRelationBusy] = useState<string | null>(null);
  const [error, setError] = useState("");
  const [working, setWorking] = useState(false);
  const [from, setFrom] = useState(() => reportDateRange("week", new Date()).from);
  const [through, setThrough] = useState(() => reportDateRange("today", new Date()).through);
  const [tokenBudget, setTokenBudget] = useState(2000);
  const [preview, setPreview] = useState("");
  const [previewType, setPreviewType] = useState<"review" | "agent" | "">("");
  const [reportDrafts, setReportDrafts] = useState<ProjectReportDraft[]>([]);
  const [reportDraftId, setReportDraftId] = useState<string | null>(null);
  const [reportCurrentRevision, setReportCurrentRevision] = useState(0);
  const [reportViewedRevision, setReportViewedRevision] = useState(0);
  const [reportBusy, setReportBusy] = useState(false);
  const [reportError, setReportError] = useState("");
  const generation = useRef(0);
  const projectGeneration = useRef(0);
  const [notice, setNotice] = useState("");
  const [onlyFlagged, setOnlyFlagged] = useState(false);
  const [sourceFilter, setSourceFilter] = useState("");
  const [agentFormat, setAgentFormat] = useState<AgentExportFormat>("markdown");
  const [selectedAgentKnowledgeIds, setSelectedAgentKnowledgeIds] = useState<string[]>([]);
  const [topicFilter, setTopicFilter] = useState<ProjectMemoryTopicFilter>("all");
  const [viewMode, setViewMode] = useState<"list" | "timeline">("list");
  const [workspaceTab, setWorkspaceTab] = useState<"knowledge" | "review" | "evolution" | "tools">("knowledge");
  const [projectRevision, setProjectRevision] = useState(0);
  const [sessionChoices, setSessionChoices] = useState<SessionItem[]>([]);
  const [sessionTotal, setSessionTotal] = useState(0);
  const [bindings, setBindings] = useState<ProjectSessionBinding[]>([]);
  const [bindingSession, setBindingSession] = useState("");
  const [bindingTarget, setBindingTarget] = useState("");
  const [bindingBusy, setBindingBusy] = useState(false);
  const [bindingError, setBindingError] = useState("");
  const [bindingNotice, setBindingNotice] = useState("");

  useEffect(() => {
    let mounted = true;
    const api = getApi();
    void Promise.all([
      api.getProjectMemories(),
      api.getSessions({ limit: 200, offset: 0 }),
      api.getProjectSessionBindings(),
    ]).then(([items, page, saved]) => {
      if (!mounted) return;
      setProjects(items);
      setSelected(current => items.some(item => item.id === current) ? current : items[0]?.id || "");
      setSessionChoices(page.items);
      setSessionTotal(page.total);
      setBindings(saved);
      setBindingSession(current => page.items.some(item => String(item.id) === current) ?
        current : String(page.items[0]?.id || ""));
      setBindingTarget(current => items.some(item => item.id === current && item.verified_path) ?
        current : items.find(item => item.verified_path)?.id || "");
    }).catch(reason => { if (mounted) setBindingError("无法读取项目归属信息：" + String(reason)); });
    return () => { mounted = false; };
  }, [projectRevision]);

  useEffect(() => {
    if (!selected) { setDetail(null); setEvolution(null); return; }
    let mounted = true;
    setError("");
    generation.current += 1;
    projectGeneration.current += 1;
    setPreview("");
    setPreviewType("");
    setReportDrafts([]);
    setReportDraftId(null);
    setReportCurrentRevision(0);
    setReportViewedRevision(0);
    setReportError("");
    setReportBusy(false);
    setNotice("");
    setWorking(false);
    setDetail(null);
    setEvolution(null);
    setEvolutionProjectId(null);
    setReviewQueueFilter("all");
    setEvolutionError("");
    setRelationBusy(null);
    setOnlyFlagged(false);
    setSourceFilter("");
    setAgentFormat("markdown");
    setSelectedAgentKnowledgeIds([]);
    setTopicFilter("all");
    setViewMode("list");
    const api = getApi();
    void api.getProjectMemory(selected).then(snapshot => {
      if (mounted) setDetail(snapshot);
    }).catch(reason => { if (mounted) setError(String(reason)); });
    void api.listProjectReportDrafts(selected).then(rows => {
      if (mounted) setReportDrafts(rows);
    }).catch(reason => {
      if (mounted) setReportError("读取本地报告草稿失败：" + String(reason));
    });
    void api.getProjectEvolution(selected).then(snapshot => {
      if (mounted) {
        setEvolution(snapshot);
        setEvolutionProjectId(selected);
      }
    }).catch(reason => {
      if (mounted) setEvolutionError(String(reason));
    });
    return () => { mounted = false; };
  }, [selected, projectRevision]);

  // Changing any export parameter invalidates old, potentially edited Markdown.
  // It also invalidates responses from a request started under older settings.
  const invalidatePreview = () => {
    generation.current += 1;
    setPreview("");
    setPreviewType("");
    setReportDraftId(null);
    setReportCurrentRevision(0);
    setReportViewedRevision(0);
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
      setEvolutionProjectId(requestProject);
      setNotice(decision === "confirmed" ? "已保存人工确认的关系审核。" : "已拒绝关系建议，原始知识不受影响。");
    } catch (reason) {
      if (requestGeneration === projectGeneration.current) {
        setEvolutionError("关系审核未完成：" + String(reason));
      }
    } finally {
      if (requestGeneration === projectGeneration.current) setRelationBusy(null);
    }
  };

  const applyProjectBinding = async (clear = false) => {
    if (bindingBusy || !bindingSession) return;
    const sessionId = Number(bindingSession);
    const session = sessionChoices.find(item => item.id === sessionId);
    const project = projects.find(item => item.id === bindingTarget);
    const message = clear
      ? `撤销 ${session?.title || session?.session_id || bindingSession} 的本地项目归属修正？原始来源信息不变。`
      : `将 ${session?.title || session?.session_id || bindingSession} 归入「${project?.title || "未知项目"}」？
仅更改 AIKS 本地记忆归组，不修改 Provider 原始文件、Session 原始路径或 SiYuan 文档。`;
    if (!window.confirm(message)) return;
    setBindingBusy(true);
    setBindingError("");
    setBindingNotice("");
    try {
      const api = getApi();
      if (clear) {
        const changed = await api.clearProjectSessionBinding(sessionId);
        setBindingNotice(changed ? "已撤销本地归属修正，恢复来源路径归组。" : "此 Session 没有本地归属修正。");
      } else {
        if (!project?.verified_path) throw new Error("必须选择有真实绝对路径的项目");
        await api.bindProjectSession(sessionId, project.id);
        setBindingNotice("已保存本地人工归属修正，可随时撤销；原始来源未改变。");
        setSelected(project.id);
      }
      setProjectRevision(value => value + 1);
    } catch (reason) {
      setBindingError("未修改项目归属：" + String(reason));
    } finally {
      setBindingBusy(false);
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
      setReportDraftId(null);
      setReportCurrentRevision(0);
      setReportViewedRevision(0);
      setPreview(kind === "agent" ? renderAgentSuggestion(output, agentFormat) : output);
    } catch (reason) {
      if (requestId === generation.current) setError(String(reason));
    } finally {
      if (requestId === generation.current) setWorking(false);
    }
  };

  const openSavedReport = async (id: string, revision?: number) => {
    if (!id || !selected || reportBusy) return;
    if (preview.trim() && !window.confirm(
      "载入历史报告会替换当前尚未保存的预览编辑，是否继续？"
    )) return;
    const activeProject = selected;
    const requestId = ++generation.current;
    setReportBusy(true);
    setReportError("");
    try {
      const saved = await getApi().getProjectReportDraft(id, revision);
      if (generation.current !== requestId || selected !== activeProject) return;
      if (saved.draft.project_id !== activeProject) {
        throw new Error("历史报告项目身份不匹配");
      }
      setFrom(saved.draft.from_date);
      setThrough(saved.draft.through_date);
      setSourceFilter(saved.draft.source_filter || "");
      setPreview(saved.body);
      setPreviewType("review");
      setReportDraftId(saved.draft.id);
      setReportCurrentRevision(saved.draft.current_revision);
      setReportViewedRevision(saved.revision);
      setNotice("已载入本地报告修订 v" + saved.revision + "，再次保存只会追加新版本。");
    } catch (reason) {
      setReportError("载入报告草稿失败：" + String(reason));
    } finally {
      if (selected === activeProject) setReportBusy(false);
    }
  };

  const saveReviewedReport = async () => {
    if (!selected || previewType !== "review" || !preview.trim() || reportBusy) return;
    if (!window.confirm(
      "保存此人工审核的回顾为 AIKS 本地草稿版本？不会改动任何 SiYuan 文档或项目文件。"
    )) return;
    const activeProject = selected;
    const requestId = generation.current;
    setReportBusy(true);
    setReportError("");
    try {
      const saved = await getApi().saveProjectReportDraft(
        reportDraftId, activeProject, from, through,
        sourceFilter || null, reportCurrentRevision, preview,
      );
      if (generation.current !== requestId || selected !== activeProject) return;
      setReportDraftId(saved.draft.id);
      setReportCurrentRevision(saved.revision);
      setReportViewedRevision(saved.revision);
      setReportDrafts(await getApi().listProjectReportDrafts(activeProject));
      setNotice("已保存本地回顾版本 v" + saved.revision + "。历史版本没有被覆盖。");
    } catch (reason) {
      if (selected === activeProject) {
        setReportError("保存报告失败：" + String(reason) + "。若版本已变化请重新载入。");
      }
    } finally {
      if (selected === activeProject) setReportBusy(false);
    }
  };

  const chooseReportRange = (kind: "today" | "yesterday" | "week" | "last7" | "month") => {
    invalidatePreview();
    const range = reportDateRange(kind, new Date());
    setFrom(range.from);
    setThrough(range.through);
  };

  const toggleAgentKnowledge = (id: string, checked: boolean) => {
    invalidatePreview();
    setSelectedAgentKnowledgeIds(current => {
      if (!checked) return current.filter(value => value !== id);
      if (current.includes(id) || current.length >= 30) return current;
      return [...current, id];
    });
  };

  const generateCuratedAgentPack = async () => {
    if (!selected || working || selectedAgentKnowledgeIds.length === 0) return;
    const requestId = ++generation.current;
    const projectId = selected;
    setError("");
    setNotice("");
    setWorking(true);
    try {
      // The backend re-reads current canonical project membership for each ID.
      // Never interpolate unverified caller-provided summary into the pack.
      const output = await getApi().createCuratedAgentContextPack(
        projectId, tokenBudget, sourceFilter || undefined, [...selectedAgentKnowledgeIds],
      );
      if (requestId !== generation.current || selected !== projectId) return;
      setPreviewType("agent");
      setPreview(renderAgentSuggestion(output, agentFormat));
    } catch (reason) {
      if (requestId === generation.current && selected === projectId) {
        setError("精选知识包生成失败：" + String(reason));
      }
    } finally {
      if (requestId === generation.current && selected === projectId) setWorking(false);
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
  const reviewQueue = detail && detail.project.id === selected
    && evolution && evolutionProjectId === selected
    ? buildProjectReviewQueue(detail, evolution)
    : null;
  const reviewCandidates = reviewQueue
    ? reviewQueue.candidates.filter(item => reviewQueueFilter === "all" || item.kind === reviewQueueFilter)
    : [];
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
    <div className="memory-workspace space-y-5 p-4 text-sm md:p-6">
      <header className="memory-hero">
        <div className="flex flex-wrap items-start justify-between gap-4">
          <div>
            <p className="text-xs font-semibold uppercase tracking-widest text-blue-600 dark:text-blue-300">AIKS / Knowledge workspace</p>
            <h1 className="mt-2 text-2xl font-semibold tracking-tight text-slate-900 dark:text-white">项目长期记忆</h1>
            <p className="mt-2 max-w-2xl text-sm leading-6 text-slate-600 dark:text-slate-300">沿项目路径汇集知识证据、跟踪知识演进，并将高频管理操作与日常阅读分离。不会只凭同名项目合并不同来源。</p>
          </div>
          <span className="rounded-full border border-blue-200 bg-blue-50 px-3 py-1 text-xs font-semibold text-blue-700 dark:border-blue-500/30 dark:bg-blue-500/10 dark:text-blue-200">本地 · 来源可追溯</span>
        </div>
        <div className="mt-5 grid grid-cols-2 gap-3 sm:grid-cols-3">
          <div className="memory-stat"><span>已发现项目</span><strong>{projects.length}</strong></div>
          <div className="memory-stat"><span>当前项目会话</span><strong>{detail?.project.session_count ?? "—"}</strong></div>
          <div className="memory-stat"><span>当前项目知识</span><strong>{detail?.project.knowledge_count ?? "—"}</strong></div>
        </div>
      </header>
      {notice && <p role="status" className="rounded bg-green-50 p-3 text-green-800">{notice}</p>}
      {error && <p role="alert" className="rounded bg-red-50 p-3 text-red-700">{error}</p>}
      <div className="memory-project-picker flex items-center gap-3">
        <label htmlFor="project-memory-select">项目</label>
        <SearchSelect label="项目" value={selected} onChange={setSelected}
          options={projects.map(item => ({ value: item.id, label: `${item.title} · ${item.session_count} 会话` }))} />
      </div>
      {projects.length === 0 && <p className="text-gray-500">目前没有可用的项目 Session，先同步本地工作记录。</p>}
      <nav aria-label="项目记忆工作区" className="flex flex-wrap gap-1 rounded-xl border border-slate-200 bg-white p-1.5 shadow-sm dark:border-slate-700 dark:bg-slate-900">
        {([
          ["knowledge", "知识概览"],
          ["review", "待复核"],
          ["evolution", "知识演进"],
          ["tools", "项目工具"],
        ] as const).map(([key, label]) => (
          <UiButton variant="outline" size="sm" key={key} type="button" aria-pressed={workspaceTab === key}
            onClick={() => setWorkspaceTab(key)}
            className={`rounded-lg px-4 py-2 text-xs font-semibold transition-colors ${workspaceTab === key
              ? "bg-slate-900 text-white shadow-sm dark:bg-sky-600"
              : "text-slate-600 hover:bg-slate-100 dark:text-slate-300 dark:hover:bg-slate-800"}`}>
            {label}
          </UiButton>
        ))}
      </nav>
      <section className={`memory-panel ${workspaceTab === "tools" ? "" : "hidden"}`}>
        <h2 className="font-semibold">人工修正项目归属（仅 AIKS 本地）</h2>
        <p className="mt-2 text-xs text-gray-500">
          仅将选中的 Session 归入一个已经发现真实绝对路径的项目，适用于无路径或识别错误的历史记录。
          原 Provider 文件、原 Session 路径与 SiYuan 文档不会修改。可撤销后恢复原始自动归组。
        </p>
        {sessionTotal > sessionChoices.length && (
          <p className="mt-2 text-xs text-amber-700">
            当前只列出最近 {sessionChoices.length} / {sessionTotal} 条会话，其他历史记录需从会话管理页核查。
          </p>
        )}
        <div className="mt-3 flex flex-wrap items-end gap-2 text-xs">
          <label className="min-w-0 flex-1">选择 Session
            <UiSelect aria-label="待调整 Session" value={bindingSession}
              onChange={event => setBindingSession(event.target.value)}
              className="mt-1 block w-full rounded border bg-transparent p-2 dark:border-gray-700">
              {sessionChoices.map(item => (
                <option key={item.id} value={item.id}>
                  {item.source} · {item.title || item.session_id}（{item.project_name || "项目未知"}）
                </option>
              ))}
            </UiSelect>
          </label>
          <label className="min-w-0 flex-1">目标项目
            <UiSelect aria-label="项目归属目标" value={bindingTarget}
              onChange={event => setBindingTarget(event.target.value)}
              className="mt-1 block w-full rounded border bg-transparent p-2 dark:border-gray-700">
              {projects.filter(item => item.verified_path).map(item => (
                <option key={item.id} value={item.id}>{item.title} · {item.sources.join("、")}</option>
              ))}
            </UiSelect>
          </label>
          <UiButton variant="outline" size="sm" type="button" disabled={bindingBusy || !bindingSession || !bindingTarget}
            onClick={() => void applyProjectBinding()}
            className="rounded border border-blue-400 px-3 py-2 text-blue-600 disabled:opacity-40">
            确认本地归属
          </UiButton>
          <UiButton variant="outline" size="sm" type="button" disabled={bindingBusy || !bindings.some(item => String(item.session_id) === bindingSession)}
            onClick={() => void applyProjectBinding(true)}
            className="rounded border px-3 py-2 text-gray-600 disabled:opacity-40 dark:border-gray-700">
            撤销归属修正
          </UiButton>
        </div>
        {bindings.some(item => String(item.session_id) === bindingSession) && (
          <p className="mt-2 text-xs text-amber-700">当前 Session 有用户手动归属记录；可使用「撤销归属修正」恢复。</p>
        )}
        {bindingError && <p role="alert" className="mt-2 text-xs text-red-600">{bindingError}</p>}
        {bindingNotice && <p role="status" className="mt-2 text-xs text-green-600">{bindingNotice}</p>}
      </section>
      {detail && (
        <>
          <section className={`memory-panel ${workspaceTab === "knowledge" ? "" : "hidden"}`}>
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
                <UiButton variant="outline" size="sm" type="button" aria-pressed={topicFilter === "all"}
                  className={`rounded border px-2 py-1 text-xs ${topicFilter === "all" ? "border-blue-500 bg-blue-50 text-blue-900" : "dark:border-gray-700"}`}
                  onClick={() => setTopicFilter("all")}>全部主题 · {detail.entries.length}</UiButton>
                {PROJECT_MEMORY_TOPICS.map(topic => (
                  <UiButton variant="outline" size="sm" type="button" key={topic.id} aria-pressed={topicFilter === topic.id}
                    className={`rounded border px-2 py-1 text-xs ${topicFilter === topic.id ? "border-blue-500 bg-blue-50 text-blue-900" : "dark:border-gray-700"}`}
                    onClick={() => setTopicFilter(topic.id)}>
                    {topic.title} · {topicCounts?.[topic.id] ?? 0}
                  </UiButton>
                ))}
              </div>
            </div>
            <div className="mt-4 flex flex-wrap items-center justify-between gap-2 text-xs">
              <label className="flex items-center gap-2">
                <input type="checkbox" checked={onlyFlagged} onChange={event => setOnlyFlagged(event.target.checked)} />
                仅查看需要人工复核的知识（基于用户反馈）
              </label>
              <UiButton variant="outline" size="sm" type="button" aria-pressed={viewMode === "timeline"}
                className="rounded border px-3 py-2 dark:border-gray-700"
                onClick={() => setViewMode(current => current === "timeline" ? "list" : "timeline")}>
                {viewMode === "timeline" ? "切换为列表" : "按知识更新时间线"}
              </UiButton>
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
          <section className={`memory-panel ${workspaceTab === "review" ? "" : "hidden"}`}>
            <h2 className="font-semibold">项目知识复核队列（明确证据）</h2>
            <p className="mt-2 text-xs text-gray-500">
              仅汇总用户明确反馈和当前项目中已有来源验证的修正、替代、解决关系。
              AIKS 不因标题相似或模型摘要自动推断矛盾、过期或事实真伪；关系确认不等于工程结论已经验证。
              任何进一步编辑、归档或审核都需要用户明确操作，不会修改 Provider 或 SiYuan 原文。
            </p>
            {!reviewQueue && !evolutionError && (
              <p className="mt-2 text-xs text-gray-500">正在加载项目的可复核证据…</p>
            )}
            {reviewQueue && (
              <>
                <div role="group" aria-label="复核线索类别" className="mt-3 flex flex-wrap gap-2 text-xs">
                  <UiButton variant="outline" size="sm" type="button" aria-pressed={reviewQueueFilter === "all"}
                    className="rounded border px-2 py-1 dark:border-gray-700"
                    onClick={() => setReviewQueueFilter("all")}>
                    所有线索 · {reviewQueue.feedbackCount + reviewQueue.relationCount}
                  </UiButton>
                  <UiButton variant="outline" size="sm" type="button" aria-pressed={reviewQueueFilter === "feedback"}
                    className="rounded border px-2 py-1 dark:border-gray-700"
                    onClick={() => setReviewQueueFilter("feedback")}>
                    用户质量反馈 · {reviewQueue.feedbackCount}
                  </UiButton>
                  <UiButton variant="outline" size="sm" type="button" aria-pressed={reviewQueueFilter === "relation"}
                    className="rounded border px-2 py-1 dark:border-gray-700"
                    onClick={() => setReviewQueueFilter("relation")}>
                    人工关系线索 · {reviewQueue.relationCount}
                  </UiButton>
                </div>
                {reviewQueue.truncated && (
                  <p role="status" className="mt-2 text-xs text-amber-700">
                    当前复核队列仅覆盖已有的有界知识/关系列表，且最多展示最近 100 条；
                    不得将无显示线索解释为项目不存在风险。
                  </p>
                )}
                <div className="mt-3 space-y-2">
                  {reviewCandidates.map(item => (
                    <article key={item.id} className="rounded border p-3 text-xs dark:border-gray-700">
                      <div className="flex flex-wrap gap-2">
                        <strong>{item.title}</strong>
                        <span className="text-gray-500">记录更新 {item.updatedAt.slice(0, 10)}</span>
                      </div>
                      <p className="mt-2 text-amber-700">{item.reason}</p>
                      <p className="mt-1 whitespace-pre-wrap break-words text-gray-500">
                        {item.evidence}
                      </p>
                      <div className="mt-2 flex flex-wrap gap-3">
                        {item.knowledgeIds.map(id => (
                          <UiButton variant="outline" size="sm" type="button" key={id} onClick={() => onOpenKnowledge(id)}
                            className="text-blue-600">查看知识 {id}</UiButton>
                        ))}
                        {item.sourceSessionIds.map(id => (
                          <UiButton variant="outline" size="sm" type="button" key={id} onClick={() => onOpenSession(id)}
                            className="text-blue-600">查看来源 Session {id}</UiButton>
                        ))}
                        {item.relationId && item.relationStatus === "suggested" && (
                          <UiButton variant="outline" size="sm" type="button" disabled={relationBusy !== null}
                            className="text-green-700 disabled:opacity-40"
                            onClick={() => void reviewEvolution(item.relationId!, "confirmed")}>
                            核对原文后确认关联
                          </UiButton>
                        )}
                      </div>
                    </article>
                  ))}
                  {reviewCandidates.length === 0 && (
                    <p className="text-xs text-gray-500">
                      当前已加载证据中没有这一类别的显式复核线索；不代表所有知识均正确或没有过期内容。
                    </p>
                  )}
                </div>
              </>
            )}
          </section>
          <section className={`memory-panel ${workspaceTab === "evolution" ? "" : "hidden"}`}>
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
                        <UiButton variant="outline" size="sm" type="button" className="text-blue-600 hover:underline"
                          onClick={() => onOpenKnowledge(link.source_id)}>
                          {link.source_title}
                        </UiButton>
                        <span className="text-gray-400">→ {relationLabel} →</span>
                        <UiButton variant="outline" size="sm" type="button" className="text-blue-600 hover:underline"
                          onClick={() => onOpenKnowledge(link.target_id)}>
                          {link.target_title}
                        </UiButton>
                      </div>
                      <p className="mt-2 whitespace-pre-wrap break-words text-gray-600 dark:text-gray-300">
                        审核证据：{link.evidence}
                      </p>
                      {link.status === "suggested" && (
                        <div className="mt-3 flex flex-wrap gap-2">
                          <UiButton variant="outline" size="sm" type="button" disabled={relationBusy !== null}
                            className="rounded border border-green-400 px-2 py-1 text-green-700 disabled:opacity-40"
                            onClick={() => void reviewEvolution(link.id, "confirmed")}>
                            {relationBusy === link.id ? "审核提交中…" : "核对原文后确认关系"}
                          </UiButton>
                          <UiButton variant="outline" size="sm" type="button" disabled={relationBusy !== null}
                            className="rounded border border-gray-300 px-2 py-1 text-gray-700 disabled:opacity-40"
                            onClick={() => void reviewEvolution(link.id, "rejected")}>
                            拒绝关系建议
                          </UiButton>
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
          <section className={`memory-panel ${workspaceTab === "tools" ? "" : "hidden"}`}>
            <h2 className="font-semibold">工作回顾与 Agent 上下文</h2>
            <p className="mt-2 text-xs text-gray-500">先生成可编辑预览，再复制使用。不会自动写入项目文件或外部服务。</p>
            <label htmlFor="project-memory-source" className="my-3 flex items-center gap-2 text-xs">
              来源范围
              <UiSelect id="project-memory-source" value={sourceFilter}
                className="rounded border bg-transparent p-2 dark:border-gray-700"
                onChange={event => {
                  invalidatePreview();
                  setSelectedAgentKnowledgeIds([]);
                  setSourceFilter(event.target.value);
                }}>
                <option value="">全部来源</option>
                {detail.project.sources.map(source => <option key={source} value={source}>{source}</option>)}
              </UiSelect>
            </label>
            <div className="my-3 flex flex-wrap items-center gap-2 text-xs">
              <UiButton variant="outline" size="sm" type="button" onClick={() => chooseReportRange("today")}
                className="rounded border px-2 py-1 dark:border-gray-700">今天（日报）</UiButton>
              <UiButton variant="outline" size="sm" type="button" onClick={() => chooseReportRange("week")}
                className="rounded border px-2 py-1 dark:border-gray-700">本周（周报）</UiButton>
              <UiButton variant="outline" size="sm" type="button" onClick={() => chooseReportRange("yesterday")}
                className="rounded-lg border border-slate-200 px-3 py-2 hover:bg-slate-100 dark:border-slate-700 dark:hover:bg-slate-800">昨天</UiButton>
              <UiButton variant="outline" size="sm" type="button" onClick={() => chooseReportRange("last7")}
                className="rounded-lg border border-slate-200 px-3 py-2 hover:bg-slate-100 dark:border-slate-700 dark:hover:bg-slate-800">最近 7 天</UiButton>
              <UiButton variant="outline" size="sm" type="button" onClick={() => chooseReportRange("month")}
                className="rounded-lg border border-slate-200 px-3 py-2 hover:bg-slate-100 dark:border-slate-700 dark:hover:bg-slate-800">本月</UiButton>
              <label>开始日期 <CalendarDateField label="回顾开始日期" value={from} onChange={value => { invalidatePreview(); setFrom(value); }} /></label>
              <label>结束日期 <CalendarDateField label="回顾结束日期" value={through} onChange={value => { invalidatePreview(); setThrough(value); }} /></label>
              <UiButton variant="outline" size="sm" disabled={working || !from || !through || from > through} onClick={() => void generate("review")} className="rounded bg-blue-600 px-3 py-2 text-white disabled:opacity-40">生成项目回顾</UiButton>
            </div>
            <div className="mb-3 space-y-2 text-xs">
              <label className="flex items-center gap-2">
                Agent 导出格式
                <UiSelect aria-label="Agent 导出格式" value={agentFormat}
                  className="rounded border bg-transparent p-2 dark:border-gray-700"
                  onChange={event => {
                    invalidatePreview();
                    setAgentFormat(event.target.value as AgentExportFormat);
                  }}>
                  {AGENT_EXPORT_FORMATS.map(format => (
                    <option key={format.value} value={format.value}>{format.label}</option>
                  ))}
                </UiSelect>
              </label>
              <p className="text-gray-500">
                AGENTS.md / CLAUDE.md 仅生成可审核的引用建议片段，不自动读取、覆盖或合并已有文件。
                切换来源、格式或预算会清空旧预览，避免错误导出。
              </p>
            </div>
            <div className="mb-3 flex flex-wrap items-center gap-2 text-xs">
              <label>上下文 Token 预算（估算）
                <UiInput aria-label="Token 预算" type="number" min={256} max={8192} value={tokenBudget}
                  onChange={e => { invalidatePreview(); setTokenBudget(Math.max(256, Math.min(8192, Number(e.target.value) || 256))); }} className="ml-2 w-24 rounded border bg-transparent p-1 dark:border-gray-700" />
              </label>
              <UiButton variant="outline" size="sm" disabled={working} onClick={() => void generate("agent")} className="rounded bg-indigo-600 px-3 py-2 text-white disabled:opacity-40">生成 Agent 上下文</UiButton>
            </div>
            <section className="mb-3 rounded border border-indigo-200 p-3 text-xs dark:border-indigo-900">
              <h3 className="font-semibold">人工精选 Agent 知识包（最多 30 条）</h3>
              <p className="mt-1 text-gray-500">
                从当前项目可追溯的知识中手工选取具体条目；不会把相似标题当作同一来源。
                标记为错误或过时的知识不可选。所有选中 ID 在后端再次核验当前项目与来源。
                仅生成待审核的 Markdown/AGENTS.md/CLAUDE.md 建议，不写入任何指令文件。
              </p>
              {detail.truncated && (
                <p className="mt-1 text-amber-700">
                  当前项目知识超过 200 条，只能从已经加载的证据中选择；不代表全部知识均已审阅。
                </p>
              )}
              <div role="group" aria-label="精选 Agent 知识来源"
                className="mt-2 max-h-48 space-y-1 overflow-auto rounded border p-2 dark:border-gray-700">
                {detail.entries.filter(item =>
                  (!sourceFilter || item.source === sourceFilter) &&
                  item.feedback_status !== "incorrect" && item.feedback_status !== "outdated"
                ).map(item => (
                  <label key={item.knowledge_id} className="flex items-center gap-2">
                    <input type="checkbox" checked={selectedAgentKnowledgeIds.includes(item.knowledge_id)}
                      disabled={working || (!selectedAgentKnowledgeIds.includes(item.knowledge_id)
                        && selectedAgentKnowledgeIds.length >= 30)}
                      onChange={event => toggleAgentKnowledge(item.knowledge_id, event.target.checked)} />
                    <span className="min-w-0 break-words">
                      {item.title} · {item.source} · {item.knowledge_id}
                    </span>
                  </label>
                ))}
              </div>
              <div className="mt-2 flex flex-wrap items-center gap-3">
                <span>已选 {selectedAgentKnowledgeIds.length} / 30 条</span>
                <UiButton variant="outline" size="sm" type="button" disabled={working || selectedAgentKnowledgeIds.length === 0}
                  onClick={() => void generateCuratedAgentPack()}
                  className="rounded border border-indigo-500 px-3 py-2 text-indigo-700 disabled:opacity-40">
                  生成所选知识的 Agent 背景预览
                </UiButton>
              </div>
            </section>
            <div className="mb-3 rounded border border-gray-200 p-3 text-xs dark:border-gray-700">
              <h3 className="font-medium">本地回顾草稿与历史修订</h3>
              <p className="mt-1 text-gray-500">
                已审核的 Markdown 可显式保存到当前 AIKS 数据库；再次保存追加修订，
                不覆盖过去的人工编辑，也不会发送到 SiYuan、邮件或 Provider。
                历史最多显示最近 100 份报告。
              </p>
              <div className="mt-2 flex flex-wrap items-center gap-2">
                <UiSelect aria-label="已保存回顾草稿" value={reportDraftId || ""}
                  className="max-w-full flex-1 rounded border bg-transparent p-2 dark:border-gray-700"
                  onChange={event => void openSavedReport(event.target.value)}
                  disabled={reportBusy}>
                  <option value="">选择已保存的本地回顾</option>
                  {reportDrafts.map(item => (
                    <option value={item.id} key={item.id}>
                      {item.from_date}～{item.through_date} · v{item.current_revision} · {item.source_filter || "全部来源"}
                    </option>
                  ))}
                </UiSelect>
                {reportDraftId && (
                  <UiSelect aria-label="回顾历史修订版本" value={reportViewedRevision}
                    className="rounded border bg-transparent p-2 dark:border-gray-700"
                    disabled={reportBusy} onChange={event => void openSavedReport(
                      reportDraftId, Number(event.target.value)
                    )}>
                    {Array.from({ length: reportCurrentRevision }, (_, index) => index + 1)
                      .map(number => <option key={number} value={number}>v{number}</option>)}
                  </UiSelect>
                )}
              </div>
              {reportError && <p className="mt-2 text-red-600" role="alert">{reportError}</p>}
            </div>
            {previewType && (
              <div className="space-y-2">
                <p className="text-xs text-gray-500">{previewType === "review" ? "回顾 Markdown" : agentFormat === "markdown" ? "Agent Markdown" : `${agentFormat === "agents" ? "AGENTS.md" : "CLAUDE.md"} 建议片段`} · 可编辑预览</p>
                <UiTextarea aria-label="导出内容预览" value={preview} onChange={e => setPreview(e.target.value)} rows={15}
                  className="w-full rounded border bg-transparent p-3 font-mono text-xs dark:border-gray-700" />
                <div className="flex flex-wrap gap-2">
                  <UiButton variant="outline" size="sm" disabled={!preview.trim()} onClick={() => void copyPreview()} className="rounded border px-3 py-2 text-xs disabled:opacity-40 dark:border-gray-700">复制审核后的 Markdown</UiButton>
                  <UiButton variant="outline" size="sm" disabled={!preview.trim()} onClick={downloadPreview} className="rounded border px-3 py-2 text-xs disabled:opacity-40 dark:border-gray-700">导出 Markdown 文件</UiButton>
                  {previewType === "review" && (
                    <UiButton variant="outline" size="sm" type="button" disabled={!preview.trim() || reportBusy}
                      onClick={() => void saveReviewedReport()}
                      className="rounded border border-emerald-500 px-3 py-2 text-xs text-emerald-700 disabled:opacity-40">
                      {reportBusy ? "保存中…" : reportDraftId
                        ? "保存为新的本地修订 v" + (reportCurrentRevision + 1)
                        : "保存到本地报告草稿"}
                    </UiButton>
                  )}
                </div>
              </div>
            )}
          </section>
        </>
      )}
    </div>
  );
}
