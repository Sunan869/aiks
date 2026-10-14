import type { PipelineSummary } from "./api/types";

export type VerifiedProcessingDetail =
  | { status: "ok"; value: PipelineSummary }
  | { status: "unavailable" };

/** The response must belong to the requested run and a real Session.
 * A rejected, malformed, or misrouted response can never authorize a retry.
 * Never echo the raw transport exception into a visible error message.
 */
export async function fetchVerifiedProcessingDetail(
  fetchDetail: (runId: string) => Promise<PipelineSummary>,
  runId: string,
): Promise<VerifiedProcessingDetail> {
  if (!runId.trim()) return { status: "unavailable" };
  try {
    const value = await fetchDetail(runId);
    if (!value || value.run_id !== runId ||
        !Number.isSafeInteger(value.session_id) || value.session_id <= 0) {
      return { status: "unavailable" };
    }
    return { status: "ok", value };
  } catch {
    return { status: "unavailable" };
  }
}
