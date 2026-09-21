/** Format display labels only; keep provider IDs unchanged in data and requests. */
export function formatSourceName(source: string | null | undefined): string {
  return source === "workbuddy" ? "WorkBuddy" : source ?? "";
}
