import { describe, expect, it } from "vitest";
import {
  appendRagDelta,
  completeRagAnswer,
  INCOMPLETE_RAG_NOTICE,
  markRagIncomplete,
  ragHistory,
  type RagUiMessage,
} from "./rag-conversation";
import type { RagAnswer } from "./api/rag";

const pending: RagUiMessage[] = [
  { id: "u1", role: "user", content: "first question" },
  { id: "a1", role: "assistant", content: "", state: "pending" },
];
const result: RagAnswer = {
  answer: "Canonical answer [1]",
  model: "local-model",
  degraded: false,
  warnings: [],
  citations: [{
    index: 1, corpus: "knowledge", entityId: "k1", chunkId: "c1",
    title: "Evidence", snippet: "proof", siyuanDocId: null, matchTypes: ["fts"],
  }],
};

describe("Ask AIKS conversation completion boundaries", () => {
  it("does not feed a streaming prefix or pending assistant into history", () => {
    const streamed = appendRagDelta(pending, "a1", "uncertain prefix");
    expect(ragHistory(streamed)).toEqual([]);
    expect(streamed[1]).toMatchObject({ state: "pending", content: "uncertain prefix" });
    expect(pending[1]).toMatchObject({ state: "pending", content: "" });
  });

  it("trusts the final canonical answer, not a streamed approximation", () => {
    const streamed = appendRagDelta(pending, "a1", "wrong preliminary text");
    const done = completeRagAnswer(streamed, "a1", result);
    expect(done[1]).toMatchObject({
      state: "complete", content: "Canonical answer [1]", citations: result.citations,
    });
    expect(ragHistory(done)).toEqual([
      { role: "user", content: "first question" },
      { role: "assistant", content: "Canonical answer [1]" },
    ]);
  });

  it("preserves visible failed prefixes but revokes provenance and future context", () => {
    const streamed = appendRagDelta(pending, "a1", "an interrupted assertion");
    const failed = markRagIncomplete(streamed, "a1");
    expect(failed[1]).toMatchObject({
      state: "incomplete", content: "an interrupted assertion",
      warnings: [INCOMPLETE_RAG_NOTICE],
    });
    expect(ragHistory(failed)).toEqual([]);
    expect((failed[1] as Extract<RagUiMessage, { role: "assistant" }>).citations).toBeUndefined();
    expect(appendRagDelta(failed, "a1", " late text")).toEqual(failed);
    expect(completeRagAnswer(failed, "a1", result)).toEqual(failed);
  });

  it("never promotes blank successful payloads to completed answers", () => {
    const noAnswer = completeRagAnswer(pending, "a1", { ...result, answer: "  " });
    expect(noAnswer[1]).toMatchObject({ state: "incomplete" });
    expect(ragHistory(noAnswer)).toEqual([]);
  });

  it("omits failed pairs without contaminating later valid turns", () => {
    const failed = markRagIncomplete(appendRagDelta(pending, "a1", "prefix"), "a1");
    const next: RagUiMessage[] = [
      ...failed,
      { id: "u2", role: "user", content: "second question" },
      { id: "a2", role: "assistant", content: "", state: "pending" },
    ];
    const done = completeRagAnswer(next, "a2", result);
    expect(ragHistory(done)).toEqual([
      { role: "user", content: "second question" },
      { role: "assistant", content: "Canonical answer [1]" },
    ]);
  });

  it("refuses stale or mismatched deltas and late model completions", () => {
    expect(appendRagDelta(pending, "other", "leak")).toEqual(pending);
    expect(completeRagAnswer(pending, "other", result)).toEqual(pending);
    const done = completeRagAnswer(pending, "a1", result);
    expect(appendRagDelta(done, "a1", "leak")).toEqual(done);
    expect(markRagIncomplete(done, "a1")).toEqual(done);
  });
});
