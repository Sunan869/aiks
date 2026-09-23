import { Channel, invoke } from "@tauri-apps/api/core";
import { getApi, shouldUseMock } from "./client";
import type { RagAnswer, RagAskRequest } from "./rag";

interface RagStreamEvent {
  type: "delta";
  text: string;
}

export interface RagStreamCallbacks {
  onDelta?: (text: string) => void;
}

/**
 * Stream only the model answer text. Retrieval and citation assembly stay in
 * the Rust RAG service, and the command still resolves to the canonical final
 * RagAnswer used by the non-streaming API.
 */
export async function askAiksProgressively(
  request: RagAskRequest,
  callbacks: RagStreamCallbacks = {},
): Promise<RagAnswer> {
  if (shouldUseMock()) {
    const result = await getApi().askAiks(request);
    if (result.answer) callbacks.onDelta?.(result.answer);
    return result;
  }

  const channel = new Channel<RagStreamEvent>();
  channel.onmessage = event => {
    if (event.type === "delta" && event.text) {
      callbacks.onDelta?.(event.text);
    }
  };

  return invoke<RagAnswer>("ask_aiks_rag_stream", {
    request,
    onDelta: channel,
  });
}
