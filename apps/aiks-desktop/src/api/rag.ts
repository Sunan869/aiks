export type RagRole = "user" | "assistant";

export interface RagTurn {
  role: RagRole;
  content: string;
}

export interface RagAskRequest {
  question: string;
  history?: RagTurn[];
  /** Exact project filter. Omit to search all projects. */
  project?: string;
}

export interface RagCitation {
  index: number;
  corpus: "knowledge" | "session";
  entityId: string;
  chunkId: string | null;
  title: string;
  snippet: string;
  siyuanDocId: string | null;
  matchTypes: string[];
}

export interface RagAnswer {
  answer: string;
  citations: RagCitation[];
  degraded: boolean;
  warnings: string[];
  model: string;
}
