/**
 * S3.4 explicit multi-source review checkpoint, not an authorization layer.
 * The authoritative Project/Session scope is always checked in Rust before
 * model invocation and again at the independent SiYuan create boundary.
 */
export function allOrganizationSourcesReviewed(
  intended: readonly string[],
  loaded: readonly string[],
  visited: readonly string[],
): boolean {
  if (!intended.length || intended.length > 5 || intended.length !== loaded.length) return false;
  if (new Set(intended).size !== intended.length || new Set(loaded).size !== loaded.length) {
    return false;
  }
  if (intended.some((id, index) => !id || id !== loaded[index])) return false;
  if (new Set(visited).size !== visited.length ||
      visited.some(id => !intended.includes(id))) return false;
  return intended.every(id => visited.includes(id));
}

/**
 * Called only when the human opens a fetched source preview. A stale/injected
 * unrelated ID can never count towards the current draft's review checkpoint.
 */
export function reviewOrganizationSource(
  intended: readonly string[],
  visited: readonly string[],
  sourceId: string,
): string[] {
  if (!sourceId || !intended.includes(sourceId) || new Set(intended).size !== intended.length) {
    throw new Error("Source identity does not belong to the reviewed organization");
  }
  return visited.includes(sourceId) ? [...visited] : [...visited, sourceId];
}
