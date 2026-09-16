// Desktop API interface — all Tauri commands go through this abstraction
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
  WorkbenchStatus,
  WorkspaceMode,
  FullStatus,
  AiStatus,
} from "./types";

export interface AiksApi {
  getOverview(): Promise<Overview>;
  getSessions(opts?: { source?: string; limit?: number; offset?: number }): Promise<SessionPage>;
  getPipelineRuns(limit?: number): Promise<PipelineSummary[]>;
  getPipelineDetail(runId: string): Promise<PipelineSummary>;
  getPipelineStats(): Promise<PipelineStats>;

  // V4 Native Knowledge Workbench
  getKnowledge(opts?: KnowledgeListOptions): Promise<KnowledgePage>;
  getKnowledgeDetail(knowledgeId: string): Promise<KnowledgeDetail>;
  createKnowledge(input: KnowledgeWriteInput): Promise<KnowledgeDetail>;
  updateKnowledge(knowledgeId: string, input: KnowledgeUpdateInput): Promise<KnowledgeDetail>;
  setKnowledgeFavorite(knowledgeId: string, favorite: boolean): Promise<KnowledgeDetail>;
  archiveKnowledge(knowledgeId: string): Promise<KnowledgeDetail>;
  restoreKnowledge(knowledgeId: string): Promise<KnowledgeDetail>;
  publishKnowledge(knowledgeId: string): Promise<PublishKnowledgeResult>;
  searchKnowledge(query: string, limit?: number): Promise<SearchResponse>;

  // V4.1 Embedded SiYuan Workbench
  getWorkbenchStatus(): Promise<WorkbenchStatus>;
  showWorkbench(mode: WorkspaceMode): Promise<void>;
  hideWorkbench(): Promise<void>;
  openSiyuanDocument(docId: string, mode: WorkspaceMode): Promise<void>;
  openSiyuanBlock(docId: string, blockId: string, mode: WorkspaceMode): Promise<void>;

  // Compatibility / operational APIs
  getFullStatus(): Promise<FullStatus>;
  getAiStatus(): Promise<AiStatus>;
  syncAndExtract(source?: string): Promise<{ discovered: number; new_count: number; updated_count: number }>;
  scanSources(source?: string): Promise<{ total: number; by_source: Record<string, number> }>;
  backfillExtractions(): Promise<{ submitted: number }>;
  syncKnowledgeToSiyuan(): Promise<{ created: number; updated: number; unchanged: number; conflict: number; failed: number }>;
  getSiyuanUrl(): Promise<string | null>;
  testAiConnection(): Promise<boolean>;
}

export function isTauriContext(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export function shouldUseMock(): boolean {
  if (import.meta.env.VITE_AIKS_MOCK === "true") return true;
  return !isTauriContext();
}
