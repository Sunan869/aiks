import type { UnifiedSearchOptions, UnifiedSearchOutcome } from "./types";
import { getApi, shouldUseMock } from "./client";

export interface SearchCallbacks {
  onProgress?: (outcome: UnifiedSearchOutcome) => void;
  signal?: AbortSignal;
}

let lastRequestId = 0;

/** Testable transport lifecycle: ignore late partials and send cancellation
 * even if the original invocation has not reached Rust yet. */
export async function runSearchRequest<T>(
  start: (id: number, progress: (value: T) => void) => Promise<T>,
  cancel: (id: number) => Promise<unknown>,
  progress?: (value: T) => void,
  signal?: AbortSignal,
): Promise<T> {
  if (signal?.aborted) throw new Error("Search cancelled");
  // Monotonic across rapid calls and webview reloads; within JS safe integers.
  const id = lastRequestId = Math.max(lastRequestId + 1, Date.now() * 1000);
  let active = true;
  const abort = () => {
    active = false;
    void cancel(id).catch(() => { /* Window shutdown may close IPC first. */ });
  };
  signal?.addEventListener("abort", abort, { once: true });
  try {
    const result = await start(id, value => {
      if (active) progress?.(value);
    });
    if (signal?.aborted) throw new Error("Search cancelled");
    return result;
  } finally {
    active = false;
    signal?.removeEventListener("abort", abort);
  }
}

/** The existing final-result API stays compatible. This optional transport
 * adds a caller-scoped Tauri channel, not a second retrieval implementation. */
export async function searchAllProgressively(
  query: string,
  options: UnifiedSearchOptions,
  callbacks: SearchCallbacks,
): Promise<UnifiedSearchOutcome> {
  if (shouldUseMock()) return getApi().searchAll(query, options);
  const { Channel, invoke } = await import("@tauri-apps/api/core");
  return runSearchRequest<UnifiedSearchOutcome>(
    (requestId, progress) => {
      const channel = new Channel<UnifiedSearchOutcome>();
      channel.onmessage = progress;
      return invoke("search_all_v42", {
        query, limit: options.limit, corpora: options.corpora,
        project: options.project, source: options.source,
        requestId, onProgress: channel,
      });
    },
    requestId => invoke("search_all_v42", { query: "", requestId, cancelOnly: true }),
    callbacks.onProgress,
    callbacks.signal,
  );
}
