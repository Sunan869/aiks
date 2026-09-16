// Tauri API implementation — wraps invoke() calls
import { invoke } from "@tauri-apps/api/core";
import type { AiksApi } from "./index";
import type {
  Overview,
  SessionPage,
  PipelineSummary,
  PipelineStats,
  KnowledgePage,
  KnowledgeDetail,
  KnowledgeListOptions,
  KnowledgeWriteInput,
  KnowledgeUpdateInput,
  PublishKnowledgeResult,
  SearchResponse,
  WorkbenchBounds,
  WorkbenchStatus,
  WorkspaceMode,
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
        knowledge_items: 0, knowledge_chunks: 0, embeddings: 0,
      })),
    ]);

    const recentKnowledge = await invoke<KnowledgePage>("list_knowledge_v4", {
      status: "active", limit: 5, offset: 0,
    }).then(r => r.items).catch(() => []);
    const activePipelines = await invoke<PipelineSummary[]>("list_pipeline_runs", { limit: 5 }).catch(() => []);

    return {
      session_count: fullStatus.scan_total,
      knowledge_count: pipelineStats.knowledge_items,
      processing_count: pipelineStats.processing,
      failed_count: pipelineStats.failed,
      ai_ready: fullStatus.ai_ready,
      ai_model: fullStatus.ai_model,
      siyuan_ready: fullStatus.siyuan_ready,
      last_sync_at: fullStatus.last_sync_at,
      recent_knowledge: recentKnowledge,
      active_pipelines: activePipelines,
    };
  }

  async getSessions(opts?: { source?: string; limit?: number; offset?: number }): Promise<SessionPage> {
    return invoke("list_sessions_v3", { source: opts?.source, limit: opts?.limit, offset: opts?.offset });
  }
  async getPipelineRuns(limit?: number): Promise<PipelineSummary[]> { return invoke("list_pipeline_runs", { limit }); }
  async getPipelineDetail(runId: string): Promise<PipelineSummary> { return invoke("get_pipeline_detail", { runId }); }
  async getPipelineStats(): Promise<PipelineStats> { return invoke("get_pipeline_stats"); }

  async getKnowledge(opts?: KnowledgeListOptions): Promise<KnowledgePage> {
    return invoke("list_knowledge_v4", {
      project: opts?.project,
      category: opts?.category,
      sourceType: opts?.sourceType,
      status: opts?.status,
      favorite: opts?.favorite,
      limit: opts?.limit,
      offset: opts?.offset,
    });
  }
  async getKnowledgeDetail(knowledgeId: string): Promise<KnowledgeDetail> { return invoke("get_knowledge_detail_v4", { knowledgeId }); }
  async createKnowledge(input: KnowledgeWriteInput): Promise<KnowledgeDetail> { return invoke("create_knowledge", { input }); }
  async updateKnowledge(knowledgeId: string, input: KnowledgeUpdateInput): Promise<KnowledgeDetail> { return invoke("update_knowledge", { knowledgeId, input }); }
  async setKnowledgeFavorite(knowledgeId: string, favorite: boolean): Promise<KnowledgeDetail> { return invoke("set_knowledge_favorite", { knowledgeId, favorite }); }
  async archiveKnowledge(knowledgeId: string): Promise<KnowledgeDetail> { return invoke("archive_knowledge", { knowledgeId }); }
  async restoreKnowledge(knowledgeId: string): Promise<KnowledgeDetail> { return invoke("restore_knowledge", { knowledgeId }); }
  async publishKnowledge(knowledgeId: string): Promise<PublishKnowledgeResult> { return invoke("publish_knowledge", { knowledgeId }); }
  async searchKnowledge(query: string, limit?: number): Promise<SearchResponse> { return invoke("search_knowledge_v4", { query, limit }); }

  async getWorkbenchStatus(): Promise<WorkbenchStatus> {
    return invoke("get_workbench_status");
  }

  async mountWorkbench(bounds: WorkbenchBounds): Promise<void> {
    await invoke("mount_workbench", {
      x: bounds.x,
      y: bounds.y,
      width: bounds.width,
      height: bounds.height,
    });
  }

  async showWorkbench(mode: WorkspaceMode): Promise<void> {
    await invoke("show_workbench");
    await invoke("show_workbench_root", { mode });
  }

  async hideWorkbench(): Promise<void> {
    await invoke("hide_workbench");
  }

  async openSiyuanDocument(docId: string, mode: WorkspaceMode): Promise<void> {
    await invoke("show_workbench");
    await invoke("set_workbench_mode", { mode });
    await invoke("open_siyuan_document", { docId });
  }

  async openSiyuanBlock(docId: string, blockId: string, mode: WorkspaceMode): Promise<void> {
    await invoke("show_workbench");
    await invoke("set_workbench_mode", { mode });
    await invoke("open_siyuan_block", { docId, blockId });
  }

  async showWorkbenchDatabase(): Promise<void> {
    await invoke("show_workbench");
    await invoke("show_workbench_database");
  }

  async showWorkbenchGraph(): Promise<void> {
    await invoke("show_workbench");
    await invoke("show_workbench_graph");
  }

  async getFullStatus(): Promise<FullStatus> { return invoke("get_full_status"); }
  async getAiStatus(): Promise<AiStatus> { return invoke("get_ai_status"); }
  async syncAndExtract(source?: string): Promise<{ discovered: number; new_count: number; updated_count: number }> { return invoke("sync_and_extract", { source }); }
  async scanSources(source?: string): Promise<{ total: number; by_source: Record<string, number> }> { return invoke("scan_sources", { source }); }
  async backfillExtractions(): Promise<{ submitted: number }> {
    const r = await invoke<{ submitted: number }>("backfill_extractions");
    return { submitted: typeof r === "number" ? r : (r?.submitted ?? 0) };
  }
  async syncKnowledgeToSiyuan(): Promise<{ created: number; updated: number; unchanged: number; conflict: number; failed: number }> { return invoke("sync_knowledge_to_siyuan"); }
  async getSiyuanUrl(): Promise<string | null> { return invoke("get_siyuan_url"); }
  async testAiConnection(): Promise<boolean> { return invoke("test_ai_connection"); }
}
