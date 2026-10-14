export type AiAssistOperation =
  | "summary"
  | "tags"
  | "category"
  | "title"
  | "key_conclusions"
  | "structure"
  | "rewrite"
  | "compare"
  | "merge_draft";

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

// Only these operations may create a persistent S3.4 reviewed audit.
// Generic V4.2 AI assistance operations (summary/title/tags/category) must
// never silently acquire the semantic meaning of a reviewed merge.
export type ReviewedOrganizationOperation = Extract<
  AiAssistOperation,
  "structure" | "rewrite" | "key_conclusions" | "compare" | "merge_draft"
>;

export function isReviewedOrganizationOperation(
  operation: AiAssistOperation,
): operation is ReviewedOrganizationOperation {
  return operation === "structure" || operation === "rewrite" ||
    operation === "key_conclusions" || operation === "compare" ||
    operation === "merge_draft";
}
