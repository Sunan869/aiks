import type { SyncAndExtractResult } from "./types";
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
  UnifiedSearchOptions,
  UnifiedSearchOutcome,
  WorkbenchBounds,
  WorkbenchStatus,
  WorkspaceMode,
  V41Diagnostics,
  FullStatus,
  AiStatus,
  ShareImportResult,
} from "./types";
import type { AiAssistInput, AiAssistSuggestion } from "./ai-assist";

export interface AiksApi {
  getOverview(): Promise<Overview>;
  getSessions(opts?: { source?: string; limit?: number; offset?: number }): Promise<SessionPage>;
  importShareUrl(url: string): Promise<ShareImportResult>;
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
  searchAll(query: string, options?: UnifiedSearchOptions): Promise<UnifiedSearchOutcome>;
  assistKnowledge(input: AiAssistInput): Promise<AiAssistSuggestion>;

  // Embedded SiYuan Workbench — cross-system coordination only
  getWorkbenchStatus(): Promise<WorkbenchStatus>;
  getV41Diagnostics(): Promise<V41Diagnostics>;
  getSessionWorkbenchDocId(sessionId: number): Promise<string | null>;
  mountWorkbench(bounds: WorkbenchBounds): Promise<void>;
  showWorkbench(mode: WorkspaceMode): Promise<void>;
  reloadWorkbench(): Promise<void>;
  hideWorkbench(): Promise<void>;
  openSiyuanDocument(docId: string, mode: WorkspaceMode): Promise<void>;
  openSiyuanBlock(docId: string, blockId: string, mode: WorkspaceMode): Promise<void>;

  // Compatibility / operational APIs
  getFullStatus(): Promise<FullStatus>;
  getAiStatus(): Promise<AiStatus>;
  syncAndExtract(source?: string): Promise<SyncAndExtractResult>;
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
  return import.meta.env.DEV && import.meta.env.VITE_AIKS_MOCK === "true";
}
