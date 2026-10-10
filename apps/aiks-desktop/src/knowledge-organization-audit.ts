import type { AiAssistOperation } from "./api/ai-assist";

/** Deterministic, user-reviewable record embedded in a new knowledge document. */
export function appendOrganizationAudit(
  draft: string,
  operation: AiAssistOperation,
  sourceIds: string[],
  confirmedAt: string,
): string {
  // Validate at runtime too: imported callers and persisted drafts bypass TypeScript types.
  const allowedOperations: readonly string[] = [
    "structure", "rewrite", "key_conclusions", "compare", "merge_draft",
  ];
  if (!allowedOperations.includes(operation)) {
    throw new Error("Unsupported knowledge organization operation");
  }
  if (draft.length > 250_000) {
    throw new Error("Reviewed knowledge draft exceeds the supported size");
  }
  if ((operation === "compare" || operation === "merge_draft") && new Set(sourceIds.map(id => id.trim())).size < 2) {
    throw new Error("Multi-source organization requires two distinct knowledge sources");
  }
  if (!draft.trim() || sourceIds.length === 0 || sourceIds.some(id => !id.trim())) {
    throw new Error("A reviewed draft and explicit source identities are required");
  }
  if (!/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$/.test(confirmedAt) || Number.isNaN(Date.parse(confirmedAt))) {
    throw new Error("Review timestamp must be a valid UTC ISO instant");
  }
  // Audit references are IDs, never free-form Markdown or injected headings.
  if (sourceIds.length > 100) {
    throw new Error("Too many source identities for one review");
  }
  const ids = sourceIds.map(id => id.trim());
  if (ids.some(id => id.length > 256 || !/^[A-Za-z0-9][A-Za-z0-9._:-]*$/.test(id))) {
    throw new Error("Invalid source knowledge identity");
  }
  const evidence = [...new Set(ids)].map(id => "- AIKS 知识：" + id).join("\n");
  return [
    draft, "", "---", "", "## AIKS 整理审核记录",
    "操作：" + operation,
    "人工确认时间（UTC）：" + confirmedAt,
    "原始知识未修改；此文档为可独立归档的人工确认副本。",
    "来源知识：", evidence,
  ].join("\n");
}
