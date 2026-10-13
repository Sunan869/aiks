import type { RagAnswer, RagCitation, RagTurn } from "./api/rag";

export type RagUiMessage =
  | { id: string; role: "user"; content: string }
  | {
      id: string;
      role: "assistant";
      content: string;
      state: "pending" | "complete" | "incomplete";
      citations?: RagCitation[];
      model?: string;
      warnings?: string[];
    };

export const INCOMPLETE_RAG_NOTICE =
  "生成已中断，以上内容只是未完成的临时片段，不是可核实的最终答复，也不会传入后续对话。";

/** Only paired, server-confirmed answers may become subsequent model history. */
export function ragHistory(messages: readonly RagUiMessage[]): RagTurn[] {
  const turns: RagTurn[] = [];
  for (let index = 0; index + 1 < messages.length; index++) {
    const user = messages[index];
    const answer = messages[index + 1];
    if (user.role !== "user" || answer.role !== "assistant"
        || answer.state !== "complete" || !answer.content.trim()) continue;
    turns.push({ role: "user", content: user.content });
    turns.push({ role: "assistant", content: answer.content });
    index++;
  }
  return turns;
}

export function appendRagDelta(
  messages: readonly RagUiMessage[], assistantId: string, delta: string,
): RagUiMessage[] {
  if (!delta) return [...messages];
  return messages.map(message => message.role === "assistant"
    && message.id === assistantId && message.state === "pending"
    ? { ...message, content: message.content + delta }
    : message);
}

export function markRagIncomplete(
  messages: readonly RagUiMessage[], assistantId: string,
): RagUiMessage[] {
  return messages.map(message => message.role === "assistant"
    && message.id === assistantId && message.state === "pending"
    ? {
        ...message,
        state: "incomplete" as const,
        citations: undefined,
        model: undefined,
        warnings: [INCOMPLETE_RAG_NOTICE],
      }
    : message);
}

/** Canonical answer replaces any streamed preview, including corrections. */
export function completeRagAnswer(
  messages: readonly RagUiMessage[], assistantId: string, result: RagAnswer,
): RagUiMessage[] {
  if (!result.answer.trim()) return markRagIncomplete(messages, assistantId);
  return messages.map(message => message.role === "assistant"
    && message.id === assistantId && message.state === "pending"
    ? {
        ...message,
        state: "complete" as const,
        content: result.answer,
        citations: result.citations,
        model: result.model,
        warnings: result.warnings,
      }
    : message);
}
