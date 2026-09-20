import { describe, expect, it } from "vitest";
import settingsPageSource from "../pages/SettingsPage.tsx?raw";
import searchDialogSource from "../components/UnifiedSearchDialog.tsx?raw";
import typesSource from "./types.ts?raw";
import searchCommandsSource from "../../src-tauri/src/search_commands.rs?raw";
import desktopLibSource from "../../src-tauri/src/lib.rs?raw";
import coreLibSource from "../../../../crates/aiks-core/src/lib.rs?raw";
import embeddingSource from "../../../../crates/aiks-core/src/pipeline/embedding_client.rs?raw";

describe("V4.2 semantic search settings and rebuild", () => {
  it("exposes embedding settings with the recommended LCO preset", () => {
    expect(settingsPageSource).toContain("embedding_enabled");
    expect(settingsPageSource).toContain("embedding_base_url");
    expect(settingsPageSource).toContain("embedding_model");
    expect(settingsPageSource).toContain("embedding_dimensions");
    expect(settingsPageSource).toContain("LCO-Embedding/LCO-Embedding-Omni-3B-2605");
    expect(settingsPageSource).toContain("bge-m3:latest");
    expect(settingsPageSource).toContain("2048");
    expect(settingsPageSource).toContain("1024");
  });

  it("wires embedding settings, connectivity testing, and historical rebuild commands", () => {
    expect(settingsPageSource).toContain("get_embedding_settings");
    expect(settingsPageSource).toContain("save_embedding_settings");
    expect(settingsPageSource).toContain("test_embedding_connection_with_settings");
    expect(settingsPageSource).toContain("rebuild_semantic_index");
    expect(settingsPageSource).toContain("semantic-index-progress");
    expect(settingsPageSource).toContain("已向量化");

    expect(desktopLibSource).toContain("mod embedding_commands");
    expect(desktopLibSource).toContain("embedding_commands::get_embedding_settings");
    expect(desktopLibSource).toContain("embedding_commands::save_embedding_settings");
    expect(desktopLibSource).toContain("embedding_commands::test_embedding_connection_with_settings");
    expect(desktopLibSource).toContain("embedding_commands::rebuild_semantic_index");
    expect(coreLibSource).toContain("pub mod semantic_index");
  });

  it("treats intentionally disabled embeddings as lexical mode rather than a degradation", () => {
    expect(typesSource).toContain("semantic_enabled?: boolean");
    expect(searchCommandsSource).toContain("semantic_enabled");
    expect(searchCommandsSource).toContain("Semantic search is disabled");
    expect(searchDialogSource).toContain("outcome.semantic_enabled");
    expect(searchDialogSource).toContain("关键词检索模式");
  });

  it("defaults new embedding configuration to LCO identity without forcing a private endpoint", () => {
    expect(embeddingSource).toContain('model: "LCO-Embedding/LCO-Embedding-Omni-3B-2605".to_string()');
    expect(embeddingSource).toContain("dimensions: Some(2048)");
    expect(embeddingSource).toContain("base_url: String::new()");
  });
});
