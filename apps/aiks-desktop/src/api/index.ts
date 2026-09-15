// V3 API interface — all Tauri commands go through this abstraction
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

export interface AiksApi {
  // V3 unified API
  getOverview(): Promise<Overview>;
  getSessions(opts?: { source?: string; limit?: number; offset?: number }): Promise<SessionPage>;
  getPipelineRuns(limit?: number): Promise<PipelineSummary[]>;
  getPipelineDetail(runId: string): Promise<PipelineSummary>;
  getPipelineStats(): Promise<PipelineStats>;
  getKnowledge(opts?: { project?: string; category?: string; limit?: number; offset?: number }): Promise<KnowledgePage>;
  searchKnowledge(query: string, limit?: number): Promise<SearchResponse>;

  // V2.5 compatibility
  getFullStatus(): Promise<FullStatus>;
  getAiStatus(): Promise<AiStatus>;
  syncAndExtract(source?: string): Promise<{ discovered: number; new_count: number; updated_count: number }>;
  scanSources(source?: string): Promise<{ total: number; by_source: Record<string, number> }>;
  backfillExtractions(): Promise<{ submitted: number }>;
  syncKnowledgeToSiyuan(): Promise<{ created: number; updated: number; unchanged: number; conflict: number; failed: number }>;
  getSiyuanUrl(): Promise<string | null>;
  openSiyuanWorkspace(): Promise<void>;
  testAiConnection(): Promise<boolean>;
}

const SIYUAN_LOOPBACK_HOSTS = new Set(["127.0.0.1", "localhost", "::1", "[::1]"]);

/**
 * Only embed the loopback URL produced by AIKS' embedded SiYuan runtime.
 * This keeps the Desktop webview from ever becoming an arbitrary remote-frame host.
 */
export function normalizeSiyuanWorkspaceUrl(raw: string | null): string | null {
  if (!raw?.trim()) return null;

  try {
    const url = new URL(raw);
    if (url.protocol !== "http:" && url.protocol !== "https:") return null;
    if (!SIYUAN_LOOPBACK_HOSTS.has(url.hostname.toLowerCase())) return null;
    return url.origin;
  } catch {
    return null;
  }
}

// Detect if running in Tauri context
export function isTauriContext(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

// Use VITE_AIKS_MOCK=true OR non-Tauri context to enable Mock API
export function shouldUseMock(): boolean {
  if (import.meta.env.VITE_AIKS_MOCK === "true") return true;
  return !isTauriContext();
}
