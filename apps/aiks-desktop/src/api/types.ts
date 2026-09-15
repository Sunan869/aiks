// V3 API types — shared between Tauri and Mock implementations

export interface Overview {
  session_count: number;
  knowledge_count: number;
  processing_count: number;
  failed_count: number;
  ai_ready: boolean;
  ai_model: string;
  siyuan_ready: boolean;
  last_sync_at: string | null;
  recent_knowledge: KnowledgeSummary[];
  active_pipelines: PipelineSummary[];
}

export interface SessionItem {
  id: number;
  source: string;
  session_id: string;
  title: string | null;
  project_name: string | null;
  project_path: string | null;
  updated_at: string | null;
  content_hash: string | null;
  run_id: string | null;
  pipeline_status: string | null;
  current_stage: string | null;
}

export interface SessionPage {
  items: SessionItem[];
  total: number;
  limit: number;
  offset: number;
}

export interface PipelineSummary {
  run_id: string;
  session_id: number;
  session_title: string | null;
  source: string;
  status: string;
  current_stage: string | null;
  pipeline_version: string;
  started_at: string | null;
  finished_at: string | null;
  error_stage: string | null;
  error_message: string | null;
  stage_runs: StageRun[];
  knowledge_count: number;
}

export interface StageRun {
  stage: string;
  status: string;
  input_count: number | null;
  output_count: number | null;
  latency_ms: number | null;
  error_message: string | null;
  detail: unknown | null;
}

export interface PipelineStats {
  total: number;
  processing: number;
  ready: number;
  raw_only: number;
  failed: number;
  knowledge_items: number;
  knowledge_chunks: number;
  embeddings: number;
}

export interface KnowledgeSummary {
  id: string;
  session_id: number;
  project_name: string | null;
  title: string;
  category: string;
  summary: string;
  tags: string;
  confidence: number;
  created_at: string;
  updated_at: string;
}

export interface KnowledgePage {
  items: KnowledgeSummary[];
  total: number;
  limit: number;
  offset: number;
}

export interface SearchResult {
  id: string;
  title: string;
  category: string;
  summary: string;
  project_name: string | null;
  tags: string;
  confidence: number;
  match_type: string;
}

export interface SearchResponse {
  results: SearchResult[];
  query: string;
  total: number;
  degraded?: boolean;
  warnings?: string[];
}

// Legacy V2.5 types kept for backward compat
export interface FullStatus {
  scan_total: number;
  scan_by_source: Record<string, number>;
  db_total: number;
  db_synced: number;
  db_pending: number;
  db_conflict: number;
  db_failed: number;
  last_sync_at: string | null;
  last_sync_discovered: number;
  last_sync_new: number;
  last_sync_updated: number;
  last_sync_failed: number;
  extraction_total: number;
  extraction_success: number;
  extraction_skipped: number;
  extraction_failed: number;
  extraction_pending: number;
  siyuan_ready: boolean;
  ai_ready: boolean;
  ai_model: string;
}

export interface AiStatus {
  enabled: boolean;
  healthy: boolean;
  model: string;
  display_name: string;
  base_url: string;
  extraction_stats: {
    total: number;
    success: number;
    skipped: number;
    failed: number;
    pending: number;
  };
}
