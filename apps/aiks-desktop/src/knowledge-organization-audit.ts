import type { AiAssistOperation } from "./api/ai-assist";

/** Deterministic, user-reviewable record embedded in a new knowledge document. */
export function appendOrganizationAudit(
  draft: string,
  operation: AiAssistOperation,
  sourceIds: string[],
  confirmedAt: string,
): string {
  if (!draft.trim() || sourceIds.length === 0 || sourceIds.some(id => !id.trim())) {
    throw new Error("A reviewed draft and explicit source identities are required");
  }
  const evidence = sourceIds.map(id => "- AIKS 知识：" + id.replace(/[\r\n]/g, " ")).join("\n");
  return [
    draft, "", "---", "", "## AIKS 整理审核记录",
    "操作：" + operation,
    "人工确认时间（UTC）：" + confirmedAt,
    "原始知识未修改；此文档为可独立归档的人工确认副本。",
    "来源知识：", evidence,
  ].join("\n");
}
