use serde::{Deserialize, Serialize};

/// Knowledge categories (spec §10)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KnowledgeCategory {
    Troubleshooting,
    Implementation,
    Design,
    Research,
    Decision,
    General,
}

impl KnowledgeCategory {
    pub fn display_name(&self) -> &str {
        match self {
            KnowledgeCategory::Troubleshooting => "故障排查",
            KnowledgeCategory::Implementation => "实现",
            KnowledgeCategory::Design => "设计",
            KnowledgeCategory::Research => "研究",
            KnowledgeCategory::Decision => "决策",
            KnowledgeCategory::General => "通用",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "troubleshooting" => Self::Troubleshooting,
            "implementation" => Self::Implementation,
            "design" => Self::Design,
            "research" => Self::Research,
            "decision" => Self::Decision,
            _ => Self::General,
        }
    }
}

/// Structured knowledge document output from AI extractor (spec §9).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeDocument {
    /// Whether this session is worth extracting
    pub worth_extracting: bool,
    /// Score 0.0-1.0 indicating knowledge value
    pub knowledge_score: f64,

    pub title: String,
    pub summary: String,

    pub project: Option<String>,
    pub category: String,
    pub tags: Vec<String>,

    // Troubleshooting specific
    pub problem: Option<String>,
    pub symptoms: Option<Vec<String>>,
    pub root_causes: Option<Vec<String>>,
    pub solutions: Option<Vec<String>>,

    // Implementation/Design specific
    pub decisions: Option<Vec<String>>,
    pub key_commands: Option<Vec<String>>,
    pub key_files: Option<Vec<String>>,
    pub todos: Option<Vec<String>>,

    /// Confidence score 0.0-1.0
    pub confidence: f64,
}

impl KnowledgeDocument {
    pub fn category_typed(&self) -> KnowledgeCategory {
        KnowledgeCategory::from_str(&self.category)
    }
}

/// Intermediate chunk summary for long sessions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkSummary {
    pub chunk_index: usize,
    pub summary: String,
    pub important_errors: Vec<String>,
    pub decisions: Vec<String>,
    pub commands: Vec<String>,
    pub files: Vec<String>,
    pub todos: Vec<String>,
}
