/// V3 AI Schema — 0~N KnowledgeItems per Session
///
/// V3 upgrade from V2.5's 1:1 extraction to 1:0~N extraction.
use serde::{Deserialize, Serialize};

/// A single extracted knowledge item (V3)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct V3KnowledgeItem {
    pub title: String,
    pub category: String,
    pub summary: String,
    pub content: String,
    pub problem: Option<String>,
    pub root_causes: Option<Vec<String>>,
    pub solutions: Option<Vec<String>>,
    pub key_commands: Option<Vec<String>>,
    pub key_files: Option<Vec<String>>,
    pub decisions: Option<Vec<String>>,
    pub tags: Vec<String>,
    pub confidence: f64,
}

/// Output from V3 AI extraction — zero to N knowledge items
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct V3ExtractionResult {
    pub session_summary: String,
    pub knowledge_score: f64,
    pub worth_extracting: bool,
    pub items: Vec<V3KnowledgeItem>,
}

impl V3ExtractionResult {
    /// Empty result for sessions not worth extracting
    pub fn skip(session_summary: &str) -> Self {
        Self {
            session_summary: session_summary.to_string(),
            knowledge_score: 0.0,
            worth_extracting: false,
            items: vec![],
        }
    }
}
