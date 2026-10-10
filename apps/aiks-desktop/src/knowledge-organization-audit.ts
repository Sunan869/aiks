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
  if (!/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$/.test(confirmedAt)
    || Number.isNaN(Date.parse(confirmedAt))
    || new Date(confirmedAt).toISOString() !== confirmedAt) {
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

/** Recheck fetched source identities; UI filtering is not a trust boundary. */
export function validateOrganizationSources(
  primary: { id: string; project_name: string | null },
  others: ReadonlyArray<{ id: string; project_name: string | null }>,
): void {
  const sources = [primary, ...others];
  if (sources.length > 100) {
    throw new Error("知识来源数量超过单次审核上限");
  }
  // Do not trust IDs returned by a stale view or injected client state.
  // The final audit is a durable document, so reject ambiguous identities
  // before invoking an AI operation or writing anything to SiYuan.
  if (sources.some(item => !/^[A-Za-z0-9][A-Za-z0-9._:-]{0,255}$/.test(item.id))) {
    throw new Error("知识来源身份无效，请重新选择");
  }
  if (new Set(sources.map(item => item.id)).size !== sources.length) {
    throw new Error("知识来源包含重复身份，请重新选择");
  }
  // A missing project on the primary item does not make conflicting known
  // projects among the additional sources safe to combine.
  const knownProjects = new Set([primary, ...others]
    .map(item => item.project_name?.trim())
    .filter((name): name is string => Boolean(name)));
  if (knownProjects.size > 1) {
    throw new Error("不能跨已知的不同项目合并知识，请重新选择来源");
  }
}

/** Snapshot the exact source metadata and text used to generate a reviewed draft. */
export function organizationSourceSnapshot(
  sources: ReadonlyArray<{
    id: string;
    title: string;
    content: string;
    summary: string | null;
    project_name: string | null;
    category: string;
    tags: string;
  }>,
): string {
  return JSON.stringify(sources.map(source => [
    source.id, source.title, source.content, source.summary,
    source.project_name, source.category, source.tags,
  ]));
}
