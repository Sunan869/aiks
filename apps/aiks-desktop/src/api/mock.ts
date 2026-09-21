import type { SyncAndExtractResult } from "./types";
// Mock API — provides realistic data for browser dev mode
import type { AiksApi } from "./index";
import type { AiAssistInput, AiAssistSuggestion } from "./ai-assist";
import type {
  Overview, SessionPage, SessionItem, PipelineSummary, PipelineStats,
  KnowledgePage, KnowledgeSummary, KnowledgeDetail, KnowledgeListOptions,
  KnowledgeWriteInput, KnowledgeUpdateInput, PublishKnowledgeResult,
  SearchResponse, UnifiedSearchOptions, UnifiedSearchOutcome,
  WorkbenchBounds, WorkbenchStatus, WorkspaceMode, V41Diagnostics, FullStatus, AiStatus, ShareImportResult,
} from "./types";

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

  async getKnowledgeDetail(knowledgeId: string): Promise<KnowledgeDetail> {
    await delay();
    const item = knowledge.find(k => k.id === knowledgeId);
    if (!item) throw new Error(`Knowledge not found: ${knowledgeId}`);
    return detailOf(item);
  }

  async createKnowledge(input: KnowledgeWriteInput): Promise<KnowledgeDetail> {
    await delay();
    const now = new Date().toISOString();
    const item: KnowledgeSummary = {
      id: `kn_manual_${Date.now()}`,
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
          .filter(k => k.status === "active" && (k.title.toLowerCase().includes(q) || k.summary.toLowerCase().includes(q)))
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
          .filter(s => (s.title ?? "").toLowerCase().includes(q) || (s.project_name ?? "").toLowerCase().includes(q))
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
    }
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
