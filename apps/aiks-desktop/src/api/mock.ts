import type { SyncAndExtractResult } from "./types";
// Mock API — provides realistic data for browser dev mode
import type { AiksApi } from "./index";
import type { AiAssistInput, AiAssistSuggestion } from "./ai-assist";
import { isReviewedOrganizationOperation } from "./ai-assist";
import type { RagAnswer, RagAskRequest } from "./rag";
import type {
  Overview, SessionPage, SessionItem, PipelineSummary, PipelineStats, TaskCenterEntry, TaskCenterStats, AiUsageStats,
  KnowledgePage, KnowledgeSummary, KnowledgeDetail, OrganizationAuditEntry, KnowledgeFeedback, KnowledgeFeedbackKind, KnowledgeRelation, KnowledgeRelationType, KnowledgeRelationStatus, KnowledgeRelationReview, KnowledgeRelationReviewHistory, ProjectOverview, ProjectMemorySnapshot, ProjectReportDraft, ProjectReportVersion, ProjectSessionBinding, ProjectEvolutionSnapshot, KnowledgeListOptions,
  KnowledgeWriteInput, KnowledgeUpdateInput, PublishKnowledgeResult,
  SearchResponse, UnifiedSearchOptions, UnifiedSearchOutcome,
  WorkbenchBounds, WorkbenchStatus, WorkspaceMode, V41Diagnostics, FullStatus, AiStatus, ShareImportResult,
} from "./types";

const feedbackHistory: KnowledgeFeedback[] = [];
const knowledgeRelations: KnowledgeRelation[] = [];
const relationReviews: KnowledgeRelationReview[] = [];
const organizationAuditHistory: OrganizationAuditEntry[] = [];
const localKnowledgeContent = new Map<string, string>();
const reportDrafts = new Map<string, ProjectReportDraft>();
const reportRevisions = new Map<string, ProjectReportVersion[]>();
let reportSequence = 0;
let nextManualKnowledgeId = 0;
const localProjectBindings = new Map<number, ProjectSessionBinding>();

const SOURCES = ["opencode", "claude_code", "codex", "gemini_cli"];
const PROJECTS = ["AIKS", "Pipeline", "Desktop", "DevOps"];

function delay(ms = 120): Promise<void> { return new Promise(r => setTimeout(r, ms)); }

function makeSession(i: number): SessionItem {
  return {
    id: i,
    source: SOURCES[i % SOURCES.length],
    session_id: `ses_${i.toString(16).padStart(4, "0")}`,
    title: `AIKS Task ${i}`,
    project_name: PROJECTS[i % PROJECTS.length],
    project_path: `/projects/${PROJECTS[i % PROJECTS.length].toLowerCase()}`,
    updated_at: new Date(Date.now() - i * 3600_000).toISOString(),
    content_hash: `hash_${i}`,
    run_id: `run_${i}`,
    pipeline_status: i % 7 === 0 ? "PROCESSING" : "READY",
    current_stage: i % 7 === 0 ? "AI_EXTRACTED" : null,
  };
}

function makeKnowledge(i: number): KnowledgeSummary {
  const manual = i % 4 === 0;
  return {
    id: `kn_${i.toString(16).padStart(4, "0")}`,
    session_id: manual ? null : i,
    project_name: PROJECTS[i % PROJECTS.length],
    title: manual ? `手工知识 ${i}` : `AI 提炼知识 ${i}`,
    category: i % 3 === 0 ? "architecture" : i % 3 === 1 ? "implementation" : "troubleshooting",
    summary: manual ? "由用户在 AIKS 中手工创建的知识。" : "从 AI 工作记录中自动提炼，并可继续在 AIKS 中编辑。",
    tags: JSON.stringify(manual ? ["manual", "AIKS"] : ["AI", "AIKS"]),
    confidence: manual ? 1 : 0.9,
    source_type: manual ? "manual" : "conversation",
    managed_by: manual ? "user" : "pipeline",
    status: i % 13 === 0 ? "archived" : "active",
    is_favorite: i % 5 === 0,
    created_at: new Date(Date.now() - i * 7200_000).toISOString(),
    updated_at: new Date(Date.now() - i * 3600_000).toISOString(),
  };
}

const sessions = Array.from({ length: 60 }, (_, i) => makeSession(i + 1));
/**
 * Mirror the Rust relation trust boundary: two distinct Provider Sessions
 * must share a verified canonical path (or a user-bound project path).
 * Matching display names alone are NOT provenance. Manual/manual relations
 * are explicit user annotations and keep their project-name requirement.
 */
function checkRelationScope(source: KnowledgeSummary, target: KnowledgeSummary): void {
  if (source.status !== "active" || target.status !== "active") {
    throw new Error("Archived or deleted knowledge cannot be linked");
  }
  const leftSession = sessions.find(item => item.id === source.session_id);
  const rightSession = sessions.find(item => item.id === target.session_id);
  if ((source.session_id !== null && !leftSession) ||
      (target.session_id !== null && !rightSession)) {
    throw new Error("Source Session is missing");
  }
  const leftPath = leftSession
    ? (localProjectBindings.get(leftSession.id)?.project_id || leftSession.project_path)
    : null;
  const rightPath = rightSession
    ? (localProjectBindings.get(rightSession.id)?.project_id || rightSession.project_path)
    : null;
  if (leftPath && rightPath) {
    if (leftPath !== rightPath) throw new Error("Cross-project relation blocked");
    return;
  }
  if (leftPath || rightPath) throw new Error("Mixed verified and unverified projects blocked");
  if (leftSession && rightSession && leftSession.id === rightSession.id) return;
  if (!leftSession && !rightSession &&
      source.project_name?.trim() && source.project_name.trim() === target.project_name?.trim()) return;
  throw new Error("Unverified cross-Session relation blocked");
}


let knowledge = Array.from({ length: 36 }, (_, i) => makeKnowledge(i + 1));
let workbenchMode: WorkspaceMode = "knowledge";

function detailOf(item: KnowledgeSummary): KnowledgeDetail {
  return {
    ...item,
    content: `# ${item.title}\n\n${item.summary}\n\n这里是可直接在 AIKS Native Editor 中维护的 Markdown 内容。`,
    source: item.source_type === "manual" ? null : "opencode",
    session_external_id: item.source_type === "manual" ? null : `ses_${item.session_id}`,
    session_title: item.source_type === "manual" ? null : "来源工作记录",
    chunks: [],
  };
}

export class MockAiksApi implements AiksApi {
  async getOverview(): Promise<Overview> {
    await delay();
    return {
      session_count: sessions.length,
      knowledge_count: knowledge.length,
      processing_count: 3,
      failed_count: 0,
      ai_ready: true,
      ai_model: "qwen3",
      siyuan_ready: true,
      last_sync_at: new Date().toISOString(),
      recent_knowledge: knowledge.filter(k => k.status === "active").slice(0, 5),
      active_pipelines: [],
    };
  }

  async getSessionSources(): Promise<string[]> { await delay(); return [...new Set(sessions.map(item => item.source))].sort(); }
  async getSessions(opts?: { source?: string; limit?: number; offset?: number }): Promise<SessionPage> {
    await delay();
    const filtered = opts?.source ? sessions.filter(s => s.source === opts.source) : sessions;
    const offset = opts?.offset ?? 0;
    const limit = opts?.limit ?? 50;
    return { items: filtered.slice(offset, offset + limit), total: filtered.length, limit, offset };
  }

  async importShareUrl(url: string): Promise<ShareImportResult> {
    await delay(250);
    const lower = url.toLowerCase();
    const source = lower.includes("claude.ai")
      ? "claude_share"
      : lower.includes("gemini")
        ? "gemini_share"
        : lower.includes("deepseek")
          ? "deepseek_share"
          : lower.includes("doubao")
            ? "doubao_share"
            : lower.includes("kimi")
              ? "kimi_share"
              : lower.includes("yb.tencent") || lower.includes("yuanbao.tencent")
                ? "yuanbao_share"
                : lower.includes("qianwen")
                  ? "qwen_share"
                  : "chatgpt_share";
    const id = sessions.length + 1;
    const item: SessionItem = {
      id,
      source,
      session_id: `share_mock_${id}`,
      title: "导入的分享会话",
      project_name: "Web Chat",
      project_path: null,
      updated_at: new Date().toISOString(),
      content_hash: `share_hash_${id}`,
      run_id: `share_run_${id}`,
      pipeline_status: "PROCESSING",
      current_stage: "PARSED",
    };
    sessions.unshift(item);
    return {
      source,
      externalSessionId: item.session_id,
      title: item.title,
      messageCount: 8,
      sessionId: id,
      pipelineQueued: 1,
    };
  }

  async getTaskCenterEntries(): Promise<TaskCenterEntry[]> { await delay(); return []; }
  async searchTaskCenterPage(): Promise<{ items: TaskCenterEntry[]; total: number }> { await delay(); return { items: [], total: 0 }; }
  async getTaskCenterStats(): Promise<TaskCenterStats> { await delay(); return { total_sessions: sessions.length, pending: 0, running: 0, cancelled: 0, sync_issues: 0, ai_issues: 0 }; }
  async getAiUsageStats(): Promise<AiUsageStats> { await delay(); return { requests: 0, measured_requests: 0, failed_requests: 0, input_tokens: null, output_tokens: null, by_model: [] }; }
  async retryFailedAiTask(): Promise<string> { throw new Error("Mock retry is not available"); }
  async runPipelineForSession(): Promise<string> { throw new Error("Mock first-run is not available"); }
  async inspectSessionDeletion(sessionId: number) { await delay(); return { session_id: sessionId, running_jobs: 0, pending_jobs: 0, knowledge_items: 0, knowledge_chunks: 0, embedding_records: 0, siyuan_documents: 0, knowledge_siyuan_documents: 0, requires_remote_verification: false, deletion_enabled: false as const }; }
  async voidSession(): Promise<{ state: string; documents_removed: number }> { await delay(); return { state: "VOIDED", documents_removed: 0 }; }
  async restoreVoidSession(): Promise<void> { await delay(); }
  async setSessionSyncExcluded(): Promise<void> { await delay(); }
  async cancelPendingAiTask(): Promise<boolean> { throw new Error("Mock cancel is not available"); }
  async getPipelineRuns(): Promise<PipelineSummary[]> { await delay(); return []; }
  async getPipelineDetail(runId: string): Promise<PipelineSummary> {
    throw new Error(`Mock pipeline detail not available: ${runId}`);
  }
  async getPipelineStats(): Promise<PipelineStats> {
    return { total: sessions.length, processing: 3, ready: 57, raw_only: 0, failed: 0, knowledge_items: knowledge.length, knowledge_chunks: 0, embeddings: 0 };
  }

  async getKnowledge(opts?: KnowledgeListOptions): Promise<KnowledgePage> {
    await delay();
    let items = [...knowledge];
    if (opts?.project) items = items.filter(k => k.project_name === opts.project);
    if (opts?.category) items = items.filter(k => k.category === opts.category);
    if (opts?.sourceType) items = items.filter(k => k.source_type === opts.sourceType);
    if (opts?.status) items = items.filter(k => k.status === opts.status);
    if (opts?.favorite !== undefined) items = items.filter(k => k.is_favorite === opts.favorite);
    items.sort((a, b) => Number(b.is_favorite) - Number(a.is_favorite) || b.updated_at.localeCompare(a.updated_at));
    const offset = opts?.offset ?? 0;
    const limit = opts?.limit ?? 50;
    return { items: items.slice(offset, offset + limit), total: items.length, limit, offset };
  }

  async getProjectMemories(): Promise<ProjectOverview[]> {
    await delay();
    const groups = new Map<string, ProjectOverview>();
    const sessionProjects = new Map<number, string>();
    // Canonical project owners determine the target title, not a recently
    // reassigned Session whose original display name belongs to another repo.
    const ordered = [...sessions].sort((a, b) =>
      Number(localProjectBindings.has(a.id)) - Number(localProjectBindings.has(b.id)) ||
      (b.updated_at || "").localeCompare(a.updated_at || "") || b.id - a.id
    );
    for (const session of ordered) {
      const key = localProjectBindings.get(session.id)?.project_id ||
        session.project_path || "unresolved:" + session.source + ":" + session.session_id;
      sessionProjects.set(session.id, key);
      const project = groups.get(key) || {
        id: key, title: session.project_name || session.title || session.session_id,
        verified_path: Boolean(session.project_path), session_count: 0, knowledge_count: 0,
        sources: [], last_updated_at: session.updated_at || "",
      };
      project.session_count++;
      if (!project.sources.includes(session.source)) project.sources.push(session.source);
      if ((session.updated_at || "") > project.last_updated_at) project.last_updated_at = session.updated_at || "";
      groups.set(key, project);
    }
    for (const item of knowledge) {
      if (item.status !== "active" || item.session_id === null) continue;
      const group = groups.get(sessionProjects.get(item.session_id) || "");
      if (group) group.knowledge_count++;
    }
    return Array.from(groups.values()).sort((a,b) => b.last_updated_at.localeCompare(a.last_updated_at));
  }
  async getProjectMemory(projectId: string): Promise<ProjectMemorySnapshot> {
    const project = (await this.getProjectMemories()).find(item => item.id === projectId);
    if (!project) throw new Error("Project not found");
    const memberIds = new Set(sessions.filter(session =>
      (localProjectBindings.get(session.id)?.project_id ||
       session.project_path || "unresolved:" + session.source + ":" + session.session_id) === projectId
    ).map(session => session.id));
    const entries = knowledge.filter(item =>
      item.status === "active" && item.session_id !== null && memberIds.has(item.session_id)
    ).slice(0, 200).map(item => ({
      knowledge_id: item.id, session_id: item.session_id ?? 0,
      source: sessions.find(session => session.id === item.session_id)?.source || "unknown", session_external_id: String(item.session_id ?? ""),
      title: item.title, category: item.category, summary: item.summary,
      updated_at: item.updated_at,
      feedback_status: feedbackHistory.filter(row => row.knowledge_id === item.id).slice(-1)[0]?.kind ?? null,
    }));
    return { project, entries, truncated: false };
  }
  async getProjectSessionBindings(): Promise<ProjectSessionBinding[]> {
    await delay();
    return Array.from(localProjectBindings.values());
  }

  async bindProjectSession(sessionId: number, targetProjectId: string): Promise<ProjectSessionBinding> {
    await delay();
    if (!sessions.some(session => session.id === sessionId)) throw new Error("Session not found");
    if (!sessions.some(session => session.project_path === targetProjectId)) {
      throw new Error("Target project must have an original verified directory");
    }
    const binding: ProjectSessionBinding = {
      session_id: sessionId, project_id: targetProjectId, updated_at: new Date().toISOString(),
    };
    localProjectBindings.set(sessionId, binding);
    return binding;
  }

  async clearProjectSessionBinding(sessionId: number): Promise<boolean> {
    await delay();
    return localProjectBindings.delete(sessionId);
  }

  async getProjectEvolution(projectId: string): Promise<ProjectEvolutionSnapshot> {
    const snapshot = await this.getProjectMemory(projectId);
    const ids = new Set(snapshot.entries.map(entry => entry.knowledge_id));
    const links = knowledgeRelations.filter(relation =>
      relation.status !== "rejected" &&
      (relation.relation_type === "corrects" || relation.relation_type === "supersedes" ||
       relation.relation_type === "resolved_by") &&
      ids.has(relation.source_id) && ids.has(relation.target_id)
    ).map(relation => ({
      id: relation.id,
      source_id: relation.source_id,
      target_id: relation.target_id,
      source_title: knowledge.find(item => item.id === relation.source_id)?.title || "",
      target_title: knowledge.find(item => item.id === relation.target_id)?.title || "",
      relation_type: relation.relation_type as "corrects" | "supersedes" | "resolved_by",
      status: relation.status as "confirmed" | "suggested",
      evidence: relation.evidence,
      updated_at: relation.updated_at,
    }));
    return { links: links.slice(0, 100), truncated: links.length > 100 };
  }

  async listProjectReportDrafts(projectId: string): Promise<ProjectReportDraft[]> {
    await delay();
    return [...reportDrafts.values()].filter(item => item.project_id === projectId)
      .sort((a, b) => b.updated_at.localeCompare(a.updated_at))
      .slice(0, 100).map(item => ({ ...item }));
  }

  async getProjectReportDraft(draftId: string, revision?: number): Promise<ProjectReportVersion> {
    await delay();
    const rows = reportRevisions.get(draftId) || [];
    const item = rows.find(value => value.revision === (revision ?? rows.length));
    if (!item) throw new Error("Report revision not found");
    return { ...item, draft: { ...reportDrafts.get(draftId)! } };
  }

  async saveProjectReportDraft(
    draftId: string | null, projectId: string, from: string, through: string,
    source: string | null, expectedRevision: number, body: string,
  ): Promise<ProjectReportVersion> {
    await delay();
    const projects = await this.getProjectMemories();
    const project = projects.find(item => item.id === projectId);
    if (!project) throw new Error("Project not found");
    if (source && !project.sources.includes(source)) throw new Error("Unknown project source");
    const parse = (date: string) => /^\d{4}-\d{2}-\d{2}$/.test(date) &&
      !Number.isNaN(new Date(date + "T00:00:00Z").getTime()) &&
      new Date(date + "T00:00:00Z").toISOString().slice(0, 10) === date;
    if (!parse(from) || !parse(through) || from > through) throw new Error("Invalid date range");
    if (!body.trim() || new TextEncoder().encode(body).length > 250000) {
      throw new Error("Invalid reviewed report length");
    }
    const previous = draftId ? reportDrafts.get(draftId) : undefined;
    if (draftId && !previous) throw new Error("Report draft not found");
    if ((previous?.current_revision ?? 0) !== expectedRevision) {
      throw new Error("Report revision changed; reload before saving");
    }
    if (previous && (previous.project_id !== projectId ||
        previous.from_date !== from || previous.through_date !== through ||
        previous.source_filter !== source)) {
      throw new Error("Saved report scope cannot change");
    }
    const now = new Date().toISOString();
    const id = draftId || "report_" + (++reportSequence);
    const draft: ProjectReportDraft = previous
      ? { ...previous, current_revision: expectedRevision + 1, updated_at: now }
      : {
        id, project_id: projectId, from_date: from, through_date: through,
        source_filter: source, current_revision: 1, created_at: now, updated_at: now,
      };
    reportDrafts.set(id, draft);
    const version: ProjectReportVersion = {
      draft: { ...draft }, revision: draft.current_revision, body, saved_at: now,
    };
    reportRevisions.set(id, [...(reportRevisions.get(id) || []), version]);
    return version;
  }

  async createProjectReview(projectId: string, from: string, through: string, source?: string): Promise<string> {
    const snapshot = await this.getProjectMemory(projectId);
    if (from > through) throw new Error("Invalid date range");
    if (source && !snapshot.project.sources.includes(source)) throw new Error("Unknown project source");
    const items = snapshot.entries.filter(item =>
      (!source || item.source === source) &&
      item.updated_at.slice(0, 10) >= from && item.updated_at.slice(0, 10) <= through
    );
    return "# " + snapshot.project.title + " · 工作回顾\n\n来源范围：" +
      (source || snapshot.project.sources.join("、")) + "\n\n" +
      items.map(item => "- " + item.title + "（" + item.knowledge_id + "）").join("\n");
  }
  async createCuratedAgentContextPack(
    projectId: string, maxTokens: number, source: string | undefined, knowledgeIds: string[],
  ): Promise<string> {
    if (knowledgeIds.length < 1 || knowledgeIds.length > 30 ||
        new Set(knowledgeIds).size !== knowledgeIds.length ||
        knowledgeIds.some(id => !id || id.length > 256)) {
      throw new Error("Select 1 to 30 unique project knowledge items");
    }
    const snapshot = await this.getProjectMemory(projectId);
    if (source && !snapshot.project.sources.includes(source)) {
      throw new Error("Selected source does not belong to the project");
    }
    const selected = knowledgeIds.map(id => {
      const entry = snapshot.entries.find(item =>
        item.knowledge_id === id && (!source || item.source === source)
      );
      if (!entry) throw new Error("Selected knowledge not in current project scope");
      if (entry.feedback_status === "incorrect" || entry.feedback_status === "outdated") {
        throw new Error("Incorrect or outdated knowledge cannot be exported");
      }
      return entry;
    });
    const budget = Math.max(256, Math.min(8192, maxTokens)) * 3;
    let result = "# " + snapshot.project.title +
      " · AIKS 精选 Agent 背景候选\n\n请人工核验来源，不直接写入 Agent 指令文件。\n\n";
    for (const item of selected) {
      const block = "## " + item.title + "\n" + item.summary +
        "\n来源：知识 " + item.knowledge_id +
        " · Session " + item.session_external_id + " (" + item.source + ")\n\n";
      if ([...result, ...block].length > budget) {
        throw new Error("Selected knowledge exceeds the configured Agent context budget");
      }
      result += block;
    }
    return result;
  }

  async createAgentContextPack(projectId: string, maxTokens: number, source?: string): Promise<string> {
    const snapshot = await this.getProjectMemory(projectId);
    if (source && !snapshot.project.sources.includes(source)) throw new Error("Unknown project source");
    const selected = snapshot.entries.filter(item =>
      (!source || item.source === source) &&
      item.feedback_status !== "incorrect" && item.feedback_status !== "outdated"
    );
    return ("# " + snapshot.project.title + " · Agent 背景预览\n\n来源范围：" +
      (source || snapshot.project.sources.join("、")) + "\n\n" +
      selected.map(item => "## " + item.title + "\n" + item.summary + "\n来源：" + item.knowledge_id).join("\n\n")).slice(0, Math.max(256, maxTokens) * 3);
  }

  async getKnowledgeDetail(knowledgeId: string): Promise<KnowledgeDetail> {
    await delay();
    const item = knowledge.find(k => k.id === knowledgeId);
    if (!item) throw new Error(`Knowledge not found: ${knowledgeId}`);
    const detail = detailOf(item);
    return { ...detail, content: localKnowledgeContent.get(item.id) ?? detail.content };
  }

  async getKnowledgeFeedback(knowledgeId: string): Promise<KnowledgeFeedback[]> {
    await delay();
    return feedbackHistory.filter(item => item.knowledge_id === knowledgeId).slice().reverse();
  }

  async addKnowledgeFeedback(knowledgeId: string, kind: KnowledgeFeedbackKind, note: string): Promise<KnowledgeFeedback> {
    await delay();
    if (!knowledge.some(item => item.id === knowledgeId)) throw new Error("Knowledge item not found");
    if (note.length > 4000) throw new Error("Feedback note exceeds 4000 characters");
    const entry: KnowledgeFeedback = {
      id: `feedback_${Date.now()}_${feedbackHistory.length}`,
      knowledge_id: knowledgeId,
      kind,
      note,
      created_at: new Date().toISOString(),
    };
    feedbackHistory.push(entry);
    return entry;
  }

  async getKnowledgeRelations(knowledgeId: string): Promise<KnowledgeRelation[]> {
    await delay();
    return knowledgeRelations.filter(item => item.source_id === knowledgeId || item.target_id === knowledgeId);
  }

  async suggestKnowledgeRelation(sourceId: string, targetId: string, relationType: KnowledgeRelationType, evidence: string): Promise<KnowledgeRelation> {
    await delay();
    if (sourceId === targetId || !evidence.trim() || evidence.length > 4000) throw new Error("Invalid knowledge relation");
    const source = knowledge.find(item => item.id === sourceId);
    const target = knowledge.find(item => item.id === targetId);
    if (!source || !target) throw new Error("Knowledge item not found");
    checkRelationScope(source, target);
    if (knowledgeRelations.some(item => item.source_id === sourceId && item.target_id === targetId && item.relation_type === relationType)) throw new Error("Duplicate relation");
    const now = new Date().toISOString();
    const relation: KnowledgeRelation = { id: "rel_" + Date.now() + "_" + knowledgeRelations.length, source_id: sourceId, target_id: targetId, relation_type: relationType, status: "suggested", evidence: evidence.trim(), confidence: null, created_at: now, updated_at: now };
    knowledgeRelations.push(relation);
    return relation;
  }

  async reviewKnowledgeRelation(relationId: string, decision: Exclude<KnowledgeRelationStatus, "suggested">): Promise<KnowledgeRelation> {
    await delay();
    const relation = knowledgeRelations.find(item => item.id === relationId);
    if (!relation) throw new Error("Knowledge relation not found");
    if (decision === "confirmed" && relation.status !== "confirmed") {
      const source = knowledge.find(item => item.id === relation.source_id);
      const target = knowledge.find(item => item.id === relation.target_id);
      if (!source || !target) throw new Error("Knowledge relation source is missing");
      checkRelationScope(source, target);
    }
    if (relation.status !== decision) {
      const now = new Date().toISOString();
      relationReviews.push({
        id: "review_" + relationReviews.length + "_" + Date.now(),
        relation_id: relation.id,
        source_id: relation.source_id,
        target_id: relation.target_id,
        previous_status: relation.status,
        next_status: decision,
        created_at: now,
      });
      relation.status = decision;
      relation.updated_at = now;
    }
    return { ...relation };
  }

  async getKnowledgeRelationReviews(knowledgeId: string): Promise<KnowledgeRelationReviewHistory> {
    await delay();
    if (!knowledgeId.trim()) throw new Error("Review history requires a knowledge identity");
    const matches = relationReviews.filter(item =>
      item.source_id === knowledgeId || item.target_id === knowledgeId
    ).slice().reverse();
    return { reviews: matches.slice(0, 100), truncated: matches.length > 100 };
  }

  async validateOrganizationScope(knowledgeIds: string[]): Promise<boolean> {
    await delay();
    if (knowledgeIds.length < 1 || knowledgeIds.length > 5 ||
        new Set(knowledgeIds).size !== knowledgeIds.length) {
      throw new Error("Organization source count or identity is invalid");
    }
    let verifiedProject: string | null = null;
    let firstSessionId: number | null = null;
    let firstSeen = false;
    for (const id of knowledgeIds) {
      const item = knowledge.find(entry => entry.id === id);
      if (!item || item.status !== "active") throw new Error("Source knowledge is not active");
      const session = sessions.find(entry => entry.id === item.session_id);
      if (item.session_id !== null && !session) throw new Error("Source Session missing");
      const projectPath = session
        ? (localProjectBindings.get(session.id)?.project_id || session.project_path)
        : null;
      if (firstSeen) {
        const sameVerified = Boolean(projectPath && verifiedProject === projectPath);
        const sameSession = Boolean(session && firstSessionId === session.id);
        if (!sameVerified && !sameSession) {
          throw new Error("Different or unverified source projects cannot be merged");
        }
      } else {
        verifiedProject = projectPath ?? null;
        firstSessionId = session?.id ?? null;
      }
      firstSeen = true;
    }
    return true;
  }

  async createOrganizedKnowledge(
    input: KnowledgeWriteInput, sourceKnowledgeIds: string[], operation: AiAssistInput["operation"],
  ): Promise<KnowledgeDetail> {
    if (!isReviewedOrganizationOperation(operation)) {
      throw new Error("Invalid organization audit operation");
    }
    if ((operation === "compare" || operation === "merge_draft") && sourceKnowledgeIds.length < 2) {
      throw new Error("A comparison requires two knowledge sources");
    }
    await this.validateOrganizationScope(sourceKnowledgeIds);
    const created = await this.createKnowledge(input);
    organizationAuditHistory.push({
      derived_knowledge_id: created.id,
      title: created.title,
      status: "active",
      operation,
      reviewed_at: new Date().toISOString(),
      source_knowledge_ids: [...sourceKnowledgeIds],
    });
    return created;
  }

  async recoverOrganizationAudit(
    derivedKnowledgeId: string, sourceKnowledgeIds: string[], operation: AiAssistInput["operation"],
  ): Promise<boolean> {
    if (!isReviewedOrganizationOperation(operation)) throw new Error("Invalid audit operation");
    await this.validateOrganizationScope(sourceKnowledgeIds);
    const derived = knowledge.find(item => item.id === derivedKnowledgeId);
    if (!derived || derived.status !== "active" ||
        derived.source_type !== "manual" || derived.managed_by !== "user" ||
        !derived.siyuan_doc_id?.trim() || !derived.generated_hash?.trim()) {
      throw new Error("Reviewed local knowledge document is missing or inactive");
    }
    const content = localKnowledgeContent.get(derivedKnowledgeId) ?? "";
    const marker = "\n## AIKS 整理审核记录\n";
    const offset = content.lastIndexOf(marker);
    if (offset < 0 || !content.slice(0, offset).endsWith("\n\n---\n")) {
      throw new Error("Existing document has no separate AIKS review footer");
    }
    const lines = content.slice(offset + marker.length).split("\n");
    if (lines[0] !== "操作：" + operation) {
      throw new Error("Review operation does not match existing document");
    }
    const stampPrefix = "人工确认时间（UTC）：";
    const timestamp = lines[1]?.startsWith(stampPrefix)
      ? lines[1].slice(stampPrefix.length) : "";
    if (!/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$/.test(timestamp) ||
        Number.isNaN(Date.parse(timestamp)) || new Date(timestamp).toISOString() !== timestamp) {
      throw new Error("Review timestamp is missing or invalid");
    }
    if (lines[2] !== "原始知识未修改；此文档为可独立归档的人工确认副本。" ||
        lines[3] !== "来源知识：") {
      throw new Error("Existing document lacks reviewed copy source declaration");
    }
    const expectedSources = sourceKnowledgeIds.map(id => "- AIKS 知识：" + id);
    if (lines.length !== 4 + expectedSources.length ||
        expectedSources.some((value, index) => lines[4 + index] !== value)) {
      throw new Error("Review source evidence does not match existing document");
    }
    if (organizationAuditHistory.some(entry => entry.derived_knowledge_id === derivedKnowledgeId)) {
      throw new Error("Organization audit already exists");
    }
    organizationAuditHistory.push({
      derived_knowledge_id: derivedKnowledgeId, title: derived.title,
      status: "active", operation, reviewed_at: new Date().toISOString(),
      source_knowledge_ids: [...sourceKnowledgeIds],
    });
    return true;
  }

  async getOrganizationAudit(sourceKnowledgeId: string): Promise<OrganizationAuditEntry[]> {
    await delay();
    return organizationAuditHistory.filter(entry => entry.source_knowledge_ids.includes(sourceKnowledgeId))
      .map(entry => ({
        ...entry,
        status: knowledge.find(item => item.id === entry.derived_knowledge_id)?.status || "missing",
      }));
  }

  async createKnowledge(input: KnowledgeWriteInput): Promise<KnowledgeDetail> {
    await delay();
    const now = new Date().toISOString();
    const id = `kn_manual_${Date.now()}_${++nextManualKnowledgeId}`;
    const item: KnowledgeSummary = {
      id,
      siyuan_doc_id: `mock-siyuan:${id}`,
      generated_hash: `mock-generated:${id}`,
      session_id: null,
      project_name: input.project_name ?? null,
      title: input.title.trim(),
      category: input.category || "general",
      summary: input.summary ?? "",
      tags: JSON.stringify(input.tags),
      confidence: 1,
      source_type: "manual",
      managed_by: "user",
      status: "active",
      is_favorite: false,
      created_at: now,
      updated_at: now,
    };
    knowledge = [item, ...knowledge];
    localKnowledgeContent.set(item.id, input.content);
    return { ...detailOf(item), content: input.content };
  }

  async updateKnowledge(knowledgeId: string, input: KnowledgeUpdateInput): Promise<KnowledgeDetail> {
    await delay();
    const index = knowledge.findIndex(k => k.id === knowledgeId);
    if (index < 0) throw new Error(`Knowledge not found: ${knowledgeId}`);
    const next: KnowledgeSummary = {
      ...knowledge[index],
      title: input.title,
      category: input.category,
      project_name: input.project_name ?? null,
      summary: input.summary,
      tags: JSON.stringify(input.tags),
      managed_by: "user",
      updated_at: new Date().toISOString(),
    };
    knowledge[index] = next;
    localKnowledgeContent.set(knowledgeId, input.content);
    return { ...detailOf(next), content: input.content };
  }

  async setKnowledgeFavorite(knowledgeId: string, favorite: boolean): Promise<KnowledgeDetail> {
    const current = await this.getKnowledgeDetail(knowledgeId);
    const index = knowledge.findIndex(k => k.id === knowledgeId);
    knowledge[index] = { ...knowledge[index], is_favorite: favorite, updated_at: new Date().toISOString() };
    return { ...current, ...knowledge[index] };
  }

  async archiveKnowledge(knowledgeId: string): Promise<KnowledgeDetail> {
    const current = await this.getKnowledgeDetail(knowledgeId);
    const index = knowledge.findIndex(k => k.id === knowledgeId);
    knowledge[index] = { ...knowledge[index], status: "archived", updated_at: new Date().toISOString() };
    return { ...current, ...knowledge[index] };
  }

  async restoreKnowledge(knowledgeId: string): Promise<KnowledgeDetail> {
    const current = await this.getKnowledgeDetail(knowledgeId);
    const index = knowledge.findIndex(k => k.id === knowledgeId);
    knowledge[index] = { ...knowledge[index], status: "active", updated_at: new Date().toISOString() };
    return { ...current, ...knowledge[index] };
  }

  async publishKnowledge(knowledgeId: string): Promise<PublishKnowledgeResult> {
    await delay(300);
    return { knowledge_id: knowledgeId, outcome: "created", target_id: `siyuan_${knowledgeId}` };
  }

  async searchKnowledge(query: string, limit?: number): Promise<SearchResponse> {
    const q = query.toLowerCase();
    const results = knowledge
      .filter(k => k.status === "active" && (k.title.toLowerCase().includes(q) || k.summary.toLowerCase().includes(q)))
      .slice(0, limit ?? 20)
      .map(k => ({ ...k, match_type: "like" }));
    return { results, query, total: results.length };
  }

  async searchAll(query: string, options?: UnifiedSearchOptions): Promise<UnifiedSearchOutcome> {
    await delay();
    const q = query.toLowerCase();
    const allowed = new Set(options?.corpora ?? ["knowledge", "session"]);
    const knowledgeHits = allowed.has("knowledge")
      ? knowledge
          .filter(k =>
            k.status === "active" &&
            (k.title.toLowerCase().includes(q) || k.summary.toLowerCase().includes(q)) &&
            (!options?.project || k.project_name === options.project) &&
            (!options?.source || (k.session_id !== null &&
              sessions.some(s => s.id === k.session_id && s.source === options.source)))
          )
          .map(k => ({
            corpus: "knowledge" as const,
            entity_id: k.id,
            chunk_id: null,
            title: k.title,
            snippet: k.summary,
            score: 0.02,
            match_types: ["lexical"],
            siyuan_doc_id: k.siyuan_doc_id ?? null,
          }))
      : [];
    const sessionHits = allowed.has("session")
      ? sessions
          .filter(s =>
            ((s.title ?? "").toLowerCase().includes(q) || (s.project_name ?? "").toLowerCase().includes(q)) &&
            (!options?.project || s.project_name === options.project) &&
            (!options?.source || s.source === options.source)
          )
          .map(s => ({
            corpus: "session" as const,
            entity_id: String(s.id),
            chunk_id: null,
            title: s.title ?? `AI Session ${s.id}`,
            snippet: `${s.source} · ${s.project_name ?? ""}`,
            score: 0.016,
            match_types: ["lexical"],
            siyuan_doc_id: null,
          }))
      : [];
    return {
      hits: [...knowledgeHits, ...sessionHits].slice(0, options?.limit ?? 30),
      degraded: false,
      warnings: [],
      semantic_enabled: false,
    };
  }

  async assistKnowledge(input: AiAssistInput): Promise<AiAssistSuggestion> {
    await delay(240);
    const base: AiAssistSuggestion = {
      operation: input.operation,
      title: null,
      summary: null,
      tags: [],
      category: null,
      text: null,
    };
    switch (input.operation) {
      case "summary": return { ...base, summary: `摘要建议：${input.title}` };
      case "tags": return { ...base, tags: ["AIKS", "knowledge", "mock"] };
      case "category": return { ...base, category: input.existingCategory ?? "general" };
      case "title": return { ...base, title: `${input.title || "知识"}（优化）` };
      case "key_conclusions": return { ...base, text: "- 关键结论一\n- 关键结论二" };
      case "structure": return { ...base, text: `# ${input.title}\n\n## 背景\n\n${input.content}` };
      case "rewrite": return { ...base, text: input.content };
      case "compare": return { ...base, text: "## 来源差异对比\n\n" + input.content };
      case "merge_draft": return { ...base, text: "## 多来源合并草稿\n\n" + input.content };
    }
  }

  async askAiks(request: RagAskRequest): Promise<RagAnswer> {
    await delay(420);
    const hits = (await this.searchAll(request.question, {
      limit: 4, project: request.project, source: request.source,
    })).hits;
    return {
      answer: hits.length
        ? `根据当前 Mock 知识库，可以从 ${hits.length} 条相关记录中找到依据。[1]`
        : "知识库中没有检索到足够依据，暂时无法基于现有知识回答这个问题。",
      citations: hits.map((hit, index) => ({
        index: index + 1,
        corpus: hit.corpus,
        entityId: hit.entity_id,
        chunkId: hit.chunk_id,
        title: hit.title,
        snippet: hit.snippet,
        siyuanDocId: hit.siyuan_doc_id,
        matchTypes: hit.match_types,
      })),
      degraded: false,
      warnings: [],
      model: "qwen3",
    };
  }

  async getWorkbenchStatus(): Promise<WorkbenchStatus> {
    return {
      available: true,
      mounted: true,
      ready: true,
      mode: workbenchMode,
      origin: "http://127.0.0.1:6812/",
      protocol_version: 1,
    };
  }

  async getV41Diagnostics(): Promise<V41Diagnostics> {
    return {
      siyuan_ready: true,
      workbench: await this.getWorkbenchStatus(),
      migration: {
        total: knowledge.length,
        pending: 0,
        migrated: knowledge.length,
        reused: 0,
        conflicts: 0,
        failed: 0,
      },
    };
  }

  async getSessionWorkbenchDocId(_sessionId: number): Promise<string | null> {
    return null;
  }

  async mountWorkbench(_bounds: WorkbenchBounds): Promise<void> {}

  async showWorkbench(mode: WorkspaceMode): Promise<void> {
    workbenchMode = mode;
  }

  async reloadWorkbench(): Promise<void> {
    await delay(20);
  }

  async hideWorkbench(): Promise<void> {}

  async openSiyuanDocument(_docId: string, mode: WorkspaceMode): Promise<void> {
    workbenchMode = mode;
  }

  async openSiyuanBlock(_docId: string, _blockId: string, mode: WorkspaceMode): Promise<void> {
    workbenchMode = mode;
  }

  async getFullStatus(): Promise<FullStatus> {
    return {
      scan_total: sessions.length,
      scan_by_source: { "OpenCode": 30, "Claude Code": 10, "Codex": 10, "Gemini CLI": 10, "WorkBuddy": 0 },
      provider_health: { "OpenCode": true, "Claude Code": true, "Codex": true, "Gemini CLI": true, "WorkBuddy": true },
      db_total: sessions.length, db_synced: 60, db_pending: 0, db_conflict: 0, db_failed: 0,
      last_sync_at: new Date().toISOString(), last_sync_discovered: 60, last_sync_new: 0, last_sync_updated: 0, last_sync_failed: 0,
      extraction_total: 60, extraction_success: 60, extraction_skipped: 0, extraction_failed: 0, extraction_pending: 0,
      siyuan_ready: true, ai_ready: true, ai_model: "qwen3",
    };
  }

  async getAiStatus(): Promise<AiStatus> {
    return { enabled: true, healthy: true, model: "qwen3", display_name: "Mock AI", base_url: "http://127.0.0.1:11434/v1", extraction_stats: { total: 60, success: 60, skipped: 0, failed: 0, pending: 0 } };
  }
  async syncAndExtract(): Promise<SyncAndExtractResult> { return { discovered: 60, new_count: 0, updated_count: 0, unchanged_count: 60, skipped_count: 0, failed_count: 0, extraction_queued: 0 }; }
  async scanSources(): Promise<{ total: number; by_source: Record<string, number> }> { return { total: 60, by_source: { opencode: 30, claude_code: 10, codex: 10, gemini_cli: 10 } }; }
  async backfillExtractions(): Promise<{ submitted: number }> { return { submitted: 0 }; }
  async syncKnowledgeToSiyuan(): Promise<{ created: number; updated: number; unchanged: number; conflict: number; failed: number }> { return { created: 0, updated: 0, unchanged: knowledge.length, conflict: 0, failed: 0 }; }
  async getSiyuanUrl(): Promise<string | null> { return "http://127.0.0.1:6812"; }
  async testAiConnection(): Promise<boolean> { return true; }
}
