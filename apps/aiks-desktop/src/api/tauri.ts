// Tauri API implementation — wraps invoke() calls
import { invoke } from "@tauri-apps/api/core";
import type { AiksApi } from "./index";
import type {
  Overview,
  SessionPage,
  PipelineSummary,
  PipelineStats,
  KnowledgePage,
  SearchResponse,
  FullStatus,
  AiStatus,
} from "./types";

export class TauriAiksApi implements AiksApi {
  async getOverview(): Promise<Overview> {
    const [fullStatus, aiStatus, pipelineStats] = await Promise.all([
      invoke<FullStatus>("get_full_status"),
      invoke<AiStatus>("get_ai_status").catch(() => null as unknown as AiStatus),
      invoke<PipelineStats>("get_pipeline_stats").catch(() => ({
        total: 0, processing: 0, ready: 0, raw_only: 0, failed: 0,
        knowledge_items: 0, knowledge_chunks: 0, embeddings: 0
      })),
    ]);

    const recentKnowledge = await invoke<{ items: unknown[] }>("list_knowledge", {
      limit: 5, offset: 0
    }).then(r => r.items).catch(() => []);

    const activePipelines = await invoke<PipelineSummary[]>("list_pipeline_runs", {
      limit: 5
    }).catch(() => []);

    return {
      session_count: fullStatus.scan_total,
      knowledge_count: pipelineStats.knowledge_items,
      processing_count: pipelineStats.processing,
      failed_count: pipelineStats.failed,
      ai_ready: fullStatus.ai_ready,
      ai_model: fullStatus.ai_model,
      siyuan_ready: fullStatus.siyuan_ready,
      last_sync_at: fullStatus.last_sync_at,
      recent_knowledge: recentKnowledge as never,
      active_pipelines: activePipelines,
    };
  }

  async getSessions(opts?: { source?: string; limit?: number; offset?: number }): Promise<SessionPage> {
    return invoke("list_sessions_v3", {
      source: opts?.source,
      limit: opts?.limit,
      offset: opts?.offset,
    });
  }

  async getPipelineRuns(limit?: number): Promise<PipelineSummary[]> {
    return invoke("list_pipeline_runs", { limit });
  }

  async getPipelineDetail(runId: string): Promise<PipelineSummary> {
    return invoke("get_pipeline_detail", { runId });
  }

  async getPipelineStats(): Promise<PipelineStats> {
    return invoke("get_pipeline_stats");
  }

  async getKnowledge(opts?: { project?: string; category?: string; limit?: number; offset?: number }): Promise<KnowledgePage> {
    return invoke("list_knowledge", {
      project: opts?.project,
      category: opts?.category,
      limit: opts?.limit,
      offset: opts?.offset,
    });
  }

  async searchKnowledge(query: string, limit?: number): Promise<SearchResponse> {
    return invoke("search_knowledge", { query, limit });
  }

  async getFullStatus(): Promise<FullStatus> {
    return invoke("get_full_status");
  }

  async getAiStatus(): Promise<AiStatus> {
    return invoke("get_ai_status");
  }

  async syncAndExtract(source?: string): Promise<{ discovered: number; new_count: number; updated_count: number }> {
    return invoke("sync_and_extract", { source });
  }

  async scanSources(source?: string): Promise<{ total: number; by_source: Record<string, number> }> {
    return invoke("scan_sources", { source });
  }

  async backfillExtractions(): Promise<{ submitted: number }> {
    // Backend returns { submitted: N } as a JSON value
    const r = await invoke<{ submitted: number }>("backfill_extractions");
    return { submitted: typeof r === "number" ? r : (r?.submitted ?? 0) };
  }

  async syncKnowledgeToSiyuan(): Promise<{ created: number; updated: number; unchanged: number; conflict: number; failed: number }> {
    return invoke("sync_knowledge_to_siyuan");
  }

  async getSiyuanUrl(): Promise<string | null> {
    return invoke("get_siyuan_url");
  }

  async testAiConnection(): Promise<boolean> {
    return invoke("test_ai_connection");
  }
}
