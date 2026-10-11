/** Suggested Agent instruction-file excerpts derived from a REVIEWED local preview.
 * These helpers only return strings / download names. They never read or write
 * existing AGENTS.md, CLAUDE.md, Provider data or SiYuan documents.
 */
export type AgentExportFormat = "markdown" | "agents" | "claude";

export const AGENT_EXPORT_FORMATS: ReadonlyArray<{
  value: AgentExportFormat;
  label: string;
}> = [
  { value: "markdown", label: "通用 Markdown 上下文" },
  { value: "agents", label: "AGENTS.md 建议片段" },
  { value: "claude", label: "CLAUDE.md 建议片段" },
];

/** Keep all extracted model/Session text as QUOTED evidence, not live rules. */
function quoteUntrustedEvidence(markdown: string): string {
  return markdown.replace(/\r\n?/g, "\n").split("\n")
    .map(line => "> " + line).join("\n");
}

export function renderAgentSuggestion(markdown: string, format: AgentExportFormat): string {
  if (format === "markdown") return markdown;
  const target = format === "agents" ? "AGENTS.md" : "CLAUDE.md";
  return [
    "## AIKS 项目背景参考（" + target + " 建议片段）",
    "",
    "以下为从 AIKS 本地知识检索生成的**待人工核对的参考资料**，",
    "不是已验证的规则或操作指令。粘贴到现有指令文件前请人工比较差异，",
    "审查原始 Session 证据，并移除过时或敏感内容；AIKS 不会修改该文件。",
    "",
    quoteUntrustedEvidence(markdown),
  ].join("\n");
}

/** Browser download is an independent suggestion, never AGENTS.md/CLAUDE.md. */
export function projectExportFileName(
  title: string | null | undefined,
  kind: "review" | "agent",
  format: AgentExportFormat,
): string {
  const project = (title || "project")
    .replace(/[\\/:*?"<>|\x00-\x1F\x7F]/g, "_")
    .replace(/[. ]+$/g, "")
    .replace(/^\.+/g, "")
    .trim()
    .slice(0, 60) || "project";
  const suffix = kind === "review" ? "review"
    : format === "agents" ? "AGENTS-suggestion"
    : format === "claude" ? "CLAUDE-suggestion"
    : "agent-context";
  return "AIKS-" + project + "-" + suffix + ".md";
}
