import { describe, expect, it } from "vitest";
import pluginSource from "../../src-tauri/resources/siyuan/data/plugins/aiks-bridge/index.js?raw";

describe("V4.2 SiYuan lifecycle bridge contract", () => {
  it("subscribes to the official SiYuan websocket event bus", () => {
    expect(pluginSource).toContain('eventBus.on("ws-main"');
    expect(pluginSource).toContain('eventBus.off("ws-main"');
  });

  it("emits document create and delete lifecycle events", () => {
    expect(pluginSource).toContain('case "create"');
    expect(pluginSource).toContain('case "removeDoc"');
    expect(pluginSource).toContain('this.emit("documentCreated"');
    expect(pluginSource).toContain('this.emit("documentDeleted"');
  });
});
