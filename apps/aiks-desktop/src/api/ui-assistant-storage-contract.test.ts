import { describe, expect, it } from "vitest";
import ask from "../components/AskAiksPanel.tsx?raw";
import storage from "../components/DataStorage.tsx?raw";

describe("assistant and storage UI workflow boundaries", () => {
  it("keeps progressive answer and filter interactions", () => {
    expect(ask).toContain("askAiksProgressively");
    expect(ask).toContain("selectedProject");
    expect(ask).toContain("selectedSource");
    expect(ask).toContain("<RadixSelectField");
    expect(ask).toContain("setMessages([])");
    expect(ask).toContain("ragHistory(messages)");
    expect(ask).toContain("activeAssistantId.current !== null");
    expect(ask).toContain("invalidateInFlight()");
    expect(ask).toContain("markRagIncomplete(current, assistantId)");
    expect(ask).toContain('message.state === "complete"');
    expect(ask).toContain("<UiButton");
  });
  it("preserves explicit storage operations and makes setup accessible", () => {
    expect(storage).toContain("pick_data_directory");
    expect(storage).toContain("set_data_storage_root");
    expect(storage).toContain('aria-label="选择 AIKS 数据存储位置"');
    expect(storage).toContain("<UiButton");
  });
});
