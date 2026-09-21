/** Format display labels only; keep provider IDs unchanged in data and requests. */
const SOURCE_LABELS: Record<string, string> = {
  workbuddy: "WorkBuddy",
  chatgpt_share: "ChatGPT",
  claude_share: "Claude",
  gemini_share: "Gemini",
};

export function formatSourceName(source: string | null | undefined): string {
  if (!source) return "";
  return SOURCE_LABELS[source] ?? source;
}
