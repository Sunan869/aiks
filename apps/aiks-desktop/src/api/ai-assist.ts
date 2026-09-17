export type AiAssistOperation =
  | "summary"
  | "tags"
  | "category"
  | "title"
  | "key_conclusions"
  | "structure"
  | "rewrite";

export interface AiAssistInput {
  siyuanDocId: string;
  operation: AiAssistOperation;
  title: string;
  content: string;
  existingSummary?: string | null;
  existingTags?: string[];
  existingCategory?: string | null;
}

export interface AiAssistSuggestion {
  operation: AiAssistOperation;
  title: string | null;
  summary: string | null;
  tags: string[];
  category: string | null;
  text: string | null;
}
