import type { AiksApi } from "./api";

/** A list and its aggregate counts form one UI read generation.
 * Treat either half failing as a failed panel; never combine different polls.
 */
export type ProcessingPanelOutcome<A, B> =
  | { status: "ok"; first: A; second: B }
  | { status: "error" };

async function readPair<A, B>(
  first: () => Promise<A>,
  second: () => Promise<B>,
): Promise<ProcessingPanelOutcome<A, B>> {
  const [a, b] = await Promise.allSettled([
    Promise.resolve().then(first),
    Promise.resolve().then(second),
  ] as const);
  if (a.status !== "fulfilled" || b.status !== "fulfilled") return { status: "error" };
  return { status: "ok", first: a.value, second: b.value };
}

export type ProcessingReaders = Pick<
  AiksApi,
  "getPipelineRuns" | "getPipelineStats" | "getTaskCenterEntries" | "getTaskCenterStats"
>;

/** Independent panels. Error objects are deliberately discarded: transport
 * messages can contain access tokens, URLs, local paths and Session content.
 */
export async function collectProcessingPanels(api: ProcessingReaders) {
  const [pipeline, tasks] = await Promise.all([
    readPair(() => api.getPipelineRuns(300), () => api.getPipelineStats()),
    readPair(() => api.getTaskCenterEntries(200), () => api.getTaskCenterStats()),
  ]);
  return { pipeline, tasks };
}

/** Prevent a polling response crossing a task mutation from reopening controls.
 * A mutation increments the generation before and after invoking native code.
 */
export function isFreshProcessingRead(
  startedGeneration: number,
  currentGeneration: number,
  mutationInFlight: boolean,
): boolean {
  return !mutationInFlight && startedGeneration === currentGeneration;
}
