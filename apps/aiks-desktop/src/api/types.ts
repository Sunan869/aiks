export interface SessionItem {
  id: number;
  source: string;
  session_id: string;
  title: string;
  project_name: string | null;
  project_path: string | null;
  updated_at: string;
  content_hash: string;
  run_id: string | null;
  pipeline_status: string;
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
  source: string;
  title: string;
  project_name: string | null;
  status: string;
  current_stage: string | null;
  started_at: string;
  updated_at: string;
  error_code: string | null;
  error_message: string | null;
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

export type KnowledgeSourceType = "conversation" | "manual";
export type KnowledgeManagedBy = "pipeline" | "user";
export type KnowledgeStatus = "active" | "archived";

export interface KnowledgeSummary {
  id: string;
  session_id: number | null;
  project_name: string | null;
  title: string;
  category: string;
  summary: string;
  tags: string;
  confidence: number;
  source_type: KnowledgeSourceType;
  managed_by: KnowledgeManagedBy;
  status: KnowledgeStatus;
  is_favorite: boolean;
  siyuan_doc_id?: string | null;
  generated_hash?: string | null;
  created_at: string;
  updated_at: string;
}

export interface KnowledgeDetail extends KnowledgeSummary {
  content: string;
  source: string | null;
  session_external_id: string | null;
  session_title: string | null;
  chunks: Array<{
    id: string;
    heading: string | null;
    chunk_index: number;
    token_count: number;
    text: string;
    has_embedding: boolean;
  }>;
}

export interface KnowledgePage {
  items: KnowledgeSummary[];
  total: number;
  limit: number;
  offset: number;
}

export interface KnowledgeListOptions {
  project?: string;
  category?: string;
  sourceType?: KnowledgeSourceType;
  status?: KnowledgeStatus;
  favorite?: boolean;
  limit?: number;
  offset?: number;
}

export interface KnowledgeWriteInput {
  title: string;
  category?: string;
  project_name?: string | null;
  summary?: string;
  content: string;
  tags: string[];
}

export interface KnowledgeUpdateInput {
  title: string;
  category: string;
  project_name?: string | null;
  summary: string;
  content: string;
  tags: string[];
}

export interface PublishKnowledgeResult {
  knowledge_id: string;
  outcome: "created" | "updated" | "unchanged" | "conflict" | string;
  target_id: string | null;
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
  source_type?: KnowledgeSourceType;
  is_favorite?: boolean;
  siyuan_doc_id?: string | null;
  siyuan_block_id?: string | null;
}

export interface SearchResponse {
  results: SearchResult[];
  query: string;
  total: number;
  degraded?: boolean;
  warnings?: string[];
}

export type WorkspaceMode = "knowledge" | "session";

export interface WorkbenchStatus {
  available: boolean;
  ready: boolean;
  mode: WorkspaceMode;
  origin: string | null;
}

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
