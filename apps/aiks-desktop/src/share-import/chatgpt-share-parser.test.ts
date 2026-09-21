import { describe, expect, it } from "vitest";
import {
  getChatGptShareId,
  parseChatGptShareHtml,
} from "./chatgpt-share-parser";

function message(id: string, role: string, text: string, createdAt: number) {
  return {
    id,
    parent: null,
    children: [],
    message: {
      id,
      author: { role },
      create_time: createdAt,
      content: {
        content_type: "text",
        parts: [text],
      },
      metadata: {},
    },
  };
}

describe("ChatGPT Share parser", () => {
  it("accepts current and legacy public share hosts", () => {
    expect(getChatGptShareId("https://chatgpt.com/share/abc-def")).toBe("abc-def");
    expect(getChatGptShareId("https://chat.openai.com/share/abc-def")).toBe("abc-def");
    expect(getChatGptShareId("https://chatgpt.com/c/private")).toBeNull();
  });

  it("finds legacy conversation data by shape instead of a fixed route", () => {
    const user = message("u1", "user", "hello", 1_700_000_000);
    const assistant = message("a1", "assistant", "world", 1_700_000_001);
    const conversation = {
      conversation_id: "shape-test",
      title: "Shape test",
      update_time: 1_700_000_002,
      model: { slug: "gpt-test" },
      mapping: { u1: user, a1: assistant },
      linear_conversation: [{ id: "u1" }, { id: "a1" }],
    };
    const payload = { unrelated: { nested: { conversation } } };
    const html =
      `<html><body><script id="__NEXT_DATA__" type="application/json">${JSON.stringify(payload)}</script></body></html>`;

    const parsed = parseChatGptShareHtml(html);
    expect(parsed.shareId).toBe("shape-test");
    expect(parsed.title).toBe("Shape test");
    expect(parsed.replies.map(reply => [reply.type, reply.statement])).toEqual([
      ["user", "hello"],
      ["assistant", "world"],
    ]);
  });
});
