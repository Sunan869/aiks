use aiks_core::pipeline::embedding_client::EmbeddingClient;
use aiks_core::{rebuild_semantic_index as rebuild_all_semantic_indexes, Config, EmbeddingConfig};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::app_state::{config_file_path, AppState};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingSettings {
    pub embedding_enabled: bool,
    pub embedding_base_url: String,
    pub embedding_model: String,
    pub embedding_dimensions: usize,
}

impl EmbeddingSettings {
    fn from_config(config: &Config) -> Self {
        Self {
            embedding_enabled: config.embedding.enabled,
            embedding_base_url: config.embedding.base_url.clone(),
            embedding_model: config.embedding.model.clone(),
            embedding_dimensions: config.embedding.dimensions.unwrap_or(0),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct EmbeddingProbeResponse {
    pub healthy: bool,
    pub dimensions: Option<usize>,
    pub message: String,
}

fn load_config() -> Result<Config, String> {
    let path = config_file_path();
    if path.exists() {
        Config::from_file(&path).map_err(|error| format!("Failed to load aiks.toml: {error}"))
    } else {
        Ok(Config::default())
    }
}

fn persist_config(config: &Config) -> Result<(), String> {
    let path = config_file_path();
    let parent = path.parent().ok_or("Invalid config path")?;
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let content = toml::to_string_pretty(config)
        .map_err(|error| format!("Failed to serialize config: {error}"))?;
    let tmp = parent.join("aiks.toml.embedding.tmp");
    std::fs::write(&tmp, content).map_err(|error| error.to_string())?;
    if let Err(first_error) = std::fs::rename(&tmp, &path) {
        if path.exists() {
            std::fs::remove_file(&path).map_err(|error| {
                format!("Failed to replace existing config after {first_error}: {error}")
            })?;
            std::fs::rename(&tmp, &path).map_err(|error| error.to_string())?;
        } else {
            return Err(first_error.to_string());
        }
    }
    Ok(())
}

fn apply_settings(config: &mut Config, settings: &EmbeddingSettings) -> Result<(), String> {
    if settings.embedding_enabled {
        if settings.embedding_base_url.trim().is_empty() {
            return Err("Embedding 服务地址不能为空".to_string());
        }
        if settings.embedding_model.trim().is_empty() {
            return Err("Embedding 模型不能为空".to_string());
        }
        if settings.embedding_dimensions == 0 {
            return Err("Embedding 向量维度必须大于 0".to_string());
        }
    }

    config.embedding.enabled = settings.embedding_enabled;
    config.embedding.base_url = settings
        .embedding_base_url
        .trim()
        .trim_end_matches('/')
        .to_string();
    config.embedding.model = settings.embedding_model.trim().to_string();
    config.embedding.dimensions =
        (settings.embedding_dimensions > 0).then_some(settings.embedding_dimensions);
    Ok(())
}

#[tauri::command]
pub async fn get_embedding_settings() -> Result<EmbeddingSettings, String> {
    Ok(EmbeddingSettings::from_config(&load_config()?))
}

#[tauri::command]
pub async fn save_embedding_settings(settings: EmbeddingSettings) -> Result<(), String> {
    let mut config = load_config()?;
    apply_settings(&mut config, &settings)?;
    persist_config(&config)
}

#[tauri::command]
pub async fn test_embedding_connection_with_settings(
    base_url: String,
    model: String,
    dimensions: usize,
    state: State<'_, AppState>,
) -> Result<EmbeddingProbeResponse, String> {
    if base_url.trim().is_empty() || model.trim().is_empty() || dimensions == 0 {
        return Ok(EmbeddingProbeResponse {
            healthy: false,
            dimensions: None,
            message: "请先填写服务地址、模型和向量维度".to_string(),
        });
    }

    let mut config: EmbeddingConfig = state
        .engine()
        .map(|engine| engine.embedding_config().clone())
        .unwrap_or_default();
    config.enabled = true;
    config.base_url = base_url.trim().trim_end_matches('/').to_string();
    config.model = model.trim().to_string();
    config.dimensions = Some(dimensions);

    let client = EmbeddingClient::new(config).map_err(|error| error.to_string())?;
    match client
        .embed_batch(vec!["AIKS semantic search connectivity test".to_string()])
        .await
    {
        Ok(vectors) => {
            let actual = vectors.first().map(Vec::len).filter(|value| *value > 0);
            match actual {
                Some(actual) if actual == dimensions => Ok(EmbeddingProbeResponse {
                    healthy: true,
                    dimensions: Some(actual),
                    message: format!("连接正常 · {actual} 维"),
                }),
                Some(actual) => Ok(EmbeddingProbeResponse {
                    healthy: false,
                    dimensions: Some(actual),
                    message: format!("向量维度不匹配：配置 {dimensions}，实际 {actual}"),
                }),
                None => Ok(EmbeddingProbeResponse {
                    healthy: false,
                    dimensions: None,
                    message: "Embedding 服务返回了空向量".to_string(),
                }),
            }
        }
        Err(error) => Ok(EmbeddingProbeResponse {
            healthy: false,
            dimensions: None,
            message: format!("连接失败：{error}"),
        }),
    }
}

#[tauri::command]
pub async fn rebuild_semantic_index(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<aiks_core::SemanticIndexRebuildStats, String> {
    let engine = state.engine().ok_or("Engine not initialized")?;
    rebuild_all_semantic_indexes(engine.as_ref(), move |progress| {
        let _ = app.emit("semantic-index-progress", progress);
    })
    .await
    .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedding_settings_map_exposed_fields_and_preserve_advanced_config() {
        let mut config = Config::default();
        config.embedding.api_key = Some("preserve-me".to_string());
        config.embedding.batch_size = 7;
        let settings = EmbeddingSettings {
            embedding_enabled: true,
            embedding_base_url: "http://example.invalid/v1/".to_string(),
            embedding_model: "bge-m3:latest".to_string(),
            embedding_dimensions: 1024,
        };

        apply_settings(&mut config, &settings).unwrap();

        assert!(config.embedding.enabled);
        assert_eq!(config.embedding.base_url, "http://example.invalid/v1");
        assert_eq!(config.embedding.model, "bge-m3:latest");
        assert_eq!(config.embedding.dimensions, Some(1024));
        assert_eq!(config.embedding.api_key.as_deref(), Some("preserve-me"));
        assert_eq!(config.embedding.batch_size, 7);
    }

    #[test]
    fn enabled_embedding_requires_complete_connection_identity() {
        let mut config = Config::default();
        let settings = EmbeddingSettings {
            embedding_enabled: true,
            embedding_base_url: String::new(),
            embedding_model: config.embedding.model.clone(),
            embedding_dimensions: 2048,
        };
        assert!(apply_settings(&mut config, &settings).is_err());
    }
}
