import { describe, expect, it } from "vitest";
import settingsPageSource from "../pages/SettingsPage.tsx?raw";
import searchDialogSource from "../components/UnifiedSearchDialog.tsx?raw";
import commandsSource from "../../src-tauri/src/commands.rs?raw";
import libSource from "../../src-tauri/src/lib.rs?raw";
import searchSource from "../../../../crates/aiks-core/src/search/mod.rs?raw";
import engineSource from "../../../../crates/aiks-core/src/engine/mod.rs?raw";
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

    expect(commandsSource).toContain("pub embedding_enabled: bool");
    expect(commandsSource).toContain("pub embedding_base_url: String");
    expect(commandsSource).toContain("pub embedding_model: String");
    expect(commandsSource).toContain("pub embedding_dimensions: usize");
  });

  it("can probe embedding connectivity and rebuild both knowledge and session vectors", () => {
    expect(settingsPageSource).toContain("test_embedding_connection_with_settings");
    expect(settingsPageSource).toContain("rebuild_semantic_index");
    expect(settingsPageSource).toContain("semantic-index-progress");
    expect(settingsPageSource).toContain("已向量化");

    expect(commandsSource).toContain("pub async fn test_embedding_connection_with_settings");
    expect(commandsSource).toContain("pub async fn rebuild_semantic_index");
    expect(libSource).toContain("commands::test_embedding_connection_with_settings");
    expect(libSource).toContain("commands::rebuild_semantic_index");
    expect(engineSource).toContain("pub async fn rebuild_semantic_index");
    expect(engineSource).toContain("KnowledgeIndexService");
    expect(engineSource).toContain("SessionIndexService");
  });

  it("treats intentionally disabled embeddings as lexical mode rather than a degradation", () => {
    expect(searchSource).toContain("pub semantic_enabled: bool");
    expect(searchSource).not.toContain("Semantic search is disabled; returning lexical results only");
    expect(searchDialogSource).toContain("outcome.semantic_enabled");
    expect(searchDialogSource).toContain("关键词检索模式");
  });

  it("defaults new embedding configuration to LCO identity without forcing a private endpoint", () => {
    expect(embeddingSource).toContain('model: "LCO-Embedding/LCO-Embedding-Omni-3B-2605".to_string()');
    expect(embeddingSource).toContain("dimensions: Some(2048)");
    expect(embeddingSource).toContain("base_url: String::new()");
  });
});
