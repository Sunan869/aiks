// Mock API — provides realistic data for browser dev mode
// Enable with: VITE_AIKS_MOCK=true npm run dev
import type { AiksApi } from "./index";
import type {
  Overview,
  SessionPage,
  SessionItem,
  PipelineSummary,
  PipelineStats,
  KnowledgePage,
  KnowledgeSummary,
  SearchResponse,
  FullStatus,
  AiStatus,
} from "./types";

const SOURCES = ["opencode", "claude_code", "codex", "gemini_cli"];
const CATEGORIES = ["troubleshooting", "architecture", "implementation", "configuration", "decision"];
const PROJECTS = ["AIKS", "Pipeline", "SiYuan-Integration", "Frontend-V3", "DevOps"];

const PIPELINE_STAGES = ["DISCOVERED", "PARSED", "NORMALIZED", "CLEANED", "LLM_CHUNKED", "AI_EXTRACTED", "KNOWLEDGE_SPLIT", "EMBED_CHUNKED", "EMBEDDED", "INDEXED", "READY"];

function rng(seed: number) {
  let s = seed;
  return () => { s = (s * 1664525 + 1013904223) & 0xffffffff; return (s >>> 0) / 0xffffffff; };
}

function makeSession(i: number): SessionItem {
  const r = rng(i * 7);
  const source = SOURCES[Math.floor(r() * SOURCES.length)];
  const project = PROJECTS[Math.floor(r() * PROJECTS.length)];
  const pipelineStatuses = ["READY", "READY", "READY", "PROCESSING", "FAILED", "RAW_ONLY"];
  const ps = pipelineStatuses[Math.floor(r() * pipelineStatuses.length)];
  const stage = ps === "PROCESSING" ? PIPELINE_STAGES[Math.floor(r() * 8)] : null;
  return {
    id: i,
    source,
    session_id: `ses_${i.toString(16).padStart(4, "0")}`,
    title: `${project} - Task ${i}`,
    project_name: project,
    project_path: `/home/user/projects/${project.toLowerCase()}`,
    updated_at: new Date(Date.now() - i * 3600 * 1000).toISOString(),
    content_hash: `hash_${i}`,
    run_id: ps ? `run_${i}` : null,
    pipeline_status: ps || null,
    current_stage: stage,
  };
}

function makeKnowledge(i: number): KnowledgeSummary {
  const r = rng(i * 13);
  const category = CATEGORIES[Math.floor(r() * CATEGORIES.length)];
  const project = PROJECTS[Math.floor(r() * PROJECTS.length)];
  const titles: Record<string, string[]> = {
    troubleshooting: ["PowerShell NativeCommandError Fix", "SiYuan Runtime 崩溃排查", "Tauri Build 失败处理", "SQLite UNIQUE 约束冲突"],
    architecture: ["Pipeline 分层设计决策", "Embedding 模块接口设计", "Provider 解耦方案"],
    implementation: ["FTS5 全文检索实现", "Mock API 层实现", "V3 数据库迁移方案"],
    configuration: ["NSIS 安装脚本优化", "Vite 多模式构建配置"],
    decision: ["V3 架构选型：sqlite-vec vs Qdrant", "SiYuan 弱化方案确认"],
  };
  const t = titles[category] || [];
  const title = t[Math.floor(r() * t.length)] || `Knowledge Item ${i}`;
  return {
    id: `kn_${i.toString(16).padStart(4, "0")}`,
    session_id: i * 3,
    project_name: project,
    title,
    category,
    summary: `关于 ${title} 的核心要点：问题根因分析、解决方案、关键命令和经验总结。`,
    tags: JSON.stringify([project, category, "AIKS"]),
    confidence: 0.75 + r() * 0.25,
    created_at: new Date(Date.now() - i * 3600 * 1000).toISOString(),
    updated_at: new Date(Date.now() - i * 1800 * 1000).toISOString(),
  };
}

function makePipelineRun(i: number): PipelineSummary {
  const session = makeSession(i);
  const stageIdx = Math.floor(rng(i)() * PIPELINE_STAGES.length);
  const status = i % 10 === 0 ? "FAILED" : i % 7 === 0 ? "PROCESSING" : "READY";
  const currentStage = status === "PROCESSING" ? PIPELINE_STAGES[stageIdx] : null;

  const stageRuns = PIPELINE_STAGES.slice(0, status === "READY" ? PIPELINE_STAGES.length : stageIdx + 1).map((s, j) => ({
    stage: s,
    status: j < stageIdx ? "SUCCESS" : j === stageIdx && status === "FAILED" ? "FAILED" : j === stageIdx && status === "PROCESSING" ? "RUNNING" : "SUCCESS",
    input_count: 140 - j * 10,
    output_count: 130 - j * 10,
    latency_ms: 50 + j * 200,
    error_message: status === "FAILED" && j === stageIdx ? "Connection timeout to AI endpoint" : null,
    detail: null,
  }));

  return {
    run_id: `run_${i}`,
    session_id: i,
    session_title: session.title,
    source: session.source,
    status,
    current_stage: currentStage,
    pipeline_version: "v3",
    started_at: new Date(Date.now() - i * 3600 * 1000).toISOString(),
    finished_at: status !== "PROCESSING" ? new Date(Date.now() - i * 1800 * 1000).toISOString() : null,
    error_stage: status === "FAILED" ? PIPELINE_STAGES[stageIdx] : null,
    error_message: status === "FAILED" ? "AI extraction failed: timeout" : null,
    stage_runs: stageRuns,
    knowledge_count: status === "READY" ? 1 + (i % 4) : 0,
  };
}

const ALL_SESSIONS = Array.from({ length: 660 }, (_, i) => makeSession(i + 1));
const ALL_KNOWLEDGE = Array.from({ length: 238 }, (_, i) => makeKnowledge(i + 1));
const ALL_PIPELINES = Array.from({ length: 200 }, (_, i) => makePipelineRun(i + 1));

function delay(ms = 300): Promise<void> {
  return new Promise(r => setTimeout(r, ms));
}

export class MockAiksApi implements AiksApi {
  async getOverview(): Promise<Overview> {
    await delay();
    return {
      session_count: 660,
      knowledge_count: 238,
      processing_count: 12,
      failed_count: 3,
      ai_ready: true,
      ai_model: "qwen3",
      siyuan_ready: true,
      last_sync_at: new Date(Date.now() - 600 * 1000).toISOString(),
      recent_knowledge: ALL_KNOWLEDGE.slice(0, 5),
      active_pipelines: ALL_PIPELINES.filter(p => p.status === "PROCESSING").slice(0, 3),
    };
  }

  async getSessions(opts?: { source?: string; limit?: number; offset?: number }): Promise<SessionPage> {
    await delay();
    let items = ALL_SESSIONS;
    if (opts?.source) {
      items = items.filter(s => s.source === opts.source);
    }
    const offset = opts?.offset ?? 0;
    const limit = opts?.limit ?? 50;
    return {
      items: items.slice(offset, offset + limit),
      total: items.length,
      limit,
      offset,
    };
  }

  async getPipelineRuns(limit?: number): Promise<PipelineSummary[]> {
    await delay();
    return ALL_PIPELINES.slice(0, limit ?? 100);
  }

  async getPipelineDetail(runId: string): Promise<PipelineSummary> {
    await delay();
    return ALL_PIPELINES.find(p => p.run_id === runId) ?? ALL_PIPELINES[0];
  }

  async getPipelineStats(): Promise<PipelineStats> {
    await delay(100);
    return {
      total: 660,
      processing: 12,
      ready: 598,
      raw_only: 47,
      failed: 3,
      knowledge_items: 238,
      knowledge_chunks: 1042,
      embeddings: 0, // Embedding not yet configured
    };
  }

  async getKnowledge(opts?: { limit?: number; offset?: number }): Promise<KnowledgePage> {
    await delay();
    const offset = opts?.offset ?? 0;
    const limit = opts?.limit ?? 50;
    return {
      items: ALL_KNOWLEDGE.slice(offset, offset + limit),
      total: ALL_KNOWLEDGE.length,
      limit,
      offset,
    };
  }

  async searchKnowledge(query: string, limit?: number): Promise<SearchResponse> {
    await delay(200);
    const q = query.toLowerCase();
    const results = ALL_KNOWLEDGE
      .filter(k => k.title.toLowerCase().includes(q) || k.summary.toLowerCase().includes(q))
      .slice(0, limit ?? 20)
      .map(k => ({
        id: k.id,
        title: k.title,
        category: k.category,
        summary: k.summary,
        project_name: k.project_name,
        tags: k.tags,
        confidence: k.confidence,
        match_type: "like",
      }));
    return { results, query, total: results.length };
  }

  async getFullStatus(): Promise<FullStatus> {
    await delay(100);
    return {
      scan_total: 660,
      scan_by_source: { OpenCode: 518, "Claude Code": 95, Codex: 32, "Gemini CLI": 15 },
      db_total: 652,
      db_synced: 640,
      db_pending: 8,
      db_conflict: 1,
      db_failed: 3,
      last_sync_at: new Date(Date.now() - 600 * 1000).toISOString(),
      last_sync_discovered: 660,
      last_sync_new: 12,
      last_sync_updated: 8,
      last_sync_failed: 0,
      extraction_total: 652,
      extraction_success: 598,
      extraction_skipped: 47,
      extraction_failed: 7,
      extraction_pending: 0,
      siyuan_ready: true,
      ai_ready: true,
      ai_model: "qwen3",
    };
  }

  async getAiStatus(): Promise<AiStatus> {
    await delay(100);
    return {
      enabled: true,
      healthy: true,
      model: "qwen3",
      display_name: "本地 AI 服务",
      base_url: "http://127.0.0.1:11434/v1",
      extraction_stats: {
        total: 652,
        success: 598,
        skipped: 47,
        failed: 7,
        pending: 0,
      },
    };
  }

  async syncAndExtract(): Promise<{ discovered: number; new_count: number; updated_count: number }> {
    await delay(1000);
    return { discovered: 660, new_count: 0, updated_count: 0 };
  }

  async scanSources(): Promise<{ total: number; by_source: Record<string, number> }> {
    await delay(500);
    return {
      total: 660,
      by_source: { OpenCode: 518, "Claude Code": 95, Codex: 32, "Gemini CLI": 15 },
    };
  }

  async backfillExtractions(): Promise<{ submitted: number }> {
    await delay(800);
    return { submitted: 0 };
  }

  async syncKnowledgeToSiyuan(): Promise<{ created: number; updated: number; unchanged: number; conflict: number; failed: number }> {
    await delay(1200);
    return { created: 3, updated: 5, unchanged: 230, conflict: 0, failed: 0 };
  }

  async getSiyuanUrl(): Promise<string | null> {
    return "http://127.0.0.1:6812";
  }

  async testAiConnection(): Promise<boolean> {
    await delay(500);
    return true;
  }
}
