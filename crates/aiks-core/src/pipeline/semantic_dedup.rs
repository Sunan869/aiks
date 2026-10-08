//! Conservative embedding + LLM semantic knowledge reconciliation.
//! Same-session duplicates can be consolidated before persistence; cross-session
//! duplicates are linked, never deleted, to retain provenance and user edits.
use std::collections::HashSet;
use tracing::warn;
use rusqlite::params;
use crate::ai::{AiClient, config::AiModelConfig, schema_v3::{V3ExtractionResult,V3KnowledgeItem}};
use crate::pipeline::{embedding_client::{EmbeddingClient,EmbeddingConfig,cosine_sim},knowledge_repo::KnowledgeRepo};
use crate::storage::StateDb;

pub struct SemanticDedup {
    embed: EmbeddingClient,
    llm: AiClient,
    model: String,
}

impl SemanticDedup {
    pub fn new(embed: EmbeddingConfig, llm: AiModelConfig) -> anyhow::Result<Option<Self>> {
        if !embed.enabled || embed.base_url.trim().is_empty() || !llm.enabled {
            return Ok(None);
        }
        Ok(Some(Self {
            model: embed.model.clone(),
            embed: EmbeddingClient::new(embed)?,
            llm: AiClient::new(llm)?,
        }))
    }

    /// Unavailable embedding/LLM degrades safely to existing deterministic dedup.
    pub async fn reconcile(
        &self, db: &StateDb, session_id: i64, result: &mut V3ExtractionResult
    ) {
        let texts: Vec<String> = result.items.iter().map(knowledge_text).collect();
        if texts.is_empty() { return; }
        let vectors = match self.embed.embed_batch(texts).await {
            Ok(v) if v.len()==result.items.len() && v.iter().all(|v| !v.is_empty()) => v,
            Ok(_) => { warn!("[DEDUP] embedding size mismatch, fallback to exact dedup"); return; }
            Err(e) => { warn!(error=%e, "[DEDUP] embedding unavailable, fallback to exact dedup"); return; }
        };
        let mut removed = HashSet::new();
        for i in 0..result.items.len() {
            if removed.contains(&i) { continue; }
            for j in i+1..result.items.len() {
                if removed.contains(&j) || result.items[i].category != result.items[j].category { continue; }
                if cosine_sim(&vectors[i],&vectors[j]) < 0.87 { continue; }
                match self.judge(&result.items[i],&result.items[j]).await {
                    Ok("same") => {
                        let duplicate = result.items[j].clone();
                        merge_item(&mut result.items[i],&duplicate);
                        removed.insert(j);
                    }
                    Ok(_) => {}
                    Err(e) => warn!(error=%e, "[DEDUP] LLM unavailable; preserve both knowledge items"),
                }
            }
        }
        if !removed.is_empty() {
            result.items = result.items.iter().enumerate()
                .filter(|(i,_)| !removed.contains(i))
                .map(|(_,v)| v.clone()).collect();
        }
        // Cross-session candidates reuse canonical indexed knowledge embeddings.
        // Record semantic relations after saving; do not mutate another session's content.
        let candidates = match KnowledgeRepo::new(db).load_all_embeddings(&self.model) {
            Ok(v) => v,
            Err(e) => { warn!(error=%e, "[DEDUP] existing embeddings unavailable"); return; }
        };
        let mut relations = Vec::new();
        for (i,item) in result.items.iter().enumerate() {
            // Re-embed after same-session merges, so candidates reflect final content.
            let vec = match self.embed.embed_batch(vec![knowledge_text(item)]).await {
                Ok(mut v) if v.len()==1 && !v[0].is_empty() => v.remove(0),
                _ => continue,
            };
            let mut scored: Vec<_> = candidates.iter()
                .map(|v| (cosine_sim(&vec,&v.vector),v))
                .filter(|(score,_)| *score >= 0.82).collect();
            scored.sort_by(|a,b| b.0.total_cmp(&a.0));
            let mut seen = HashSet::new();
            for (score,candidate) in scored.into_iter().take(12) {
                if !seen.insert(candidate.knowledge_id.clone()) { continue; }
                let candidate_item = V3KnowledgeItem {
                    title: candidate.chunk_text.chars().take(100).collect(),
                    category: item.category.clone(),
                    summary: candidate.chunk_text.clone(),
                    content: candidate.chunk_text.clone(),
                    problem: None, root_causes: None, solutions: None,
                    key_commands: None, key_files: None, decisions: None,
                    tags: vec![], confidence: 0.0,
                };
                if let Ok(decision) = self.judge(item,&candidate_item).await {
                    if decision=="same" || decision=="related" {
                        relations.push((i,candidate.knowledge_id.clone(),decision.to_owned(),score));
                    }
                }
            }
        }
        if relations.is_empty() { return; }
        // Relations are staged by session and list index. A later store() matches
        // persisted IDs by the current item ordering in knowledge_item.
        let conn=db.conn();
        if let Err(e)=conn.execute_batch("CREATE TABLE IF NOT EXISTS semantic_knowledge_relation (
            source_session_id INTEGER NOT NULL,
            item_title TEXT NOT NULL,
            candidate_knowledge_id TEXT NOT NULL,
            relation TEXT NOT NULL,
            score REAL NOT NULL,
            embedding_model TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            PRIMARY KEY (source_session_id,item_title,candidate_knowledge_id)
        )") {
            warn!(error=%e,"[DEDUP] relation table unavailable");
            return;
        }
        for (idx,id,decision,score) in relations {
            if let Err(e)=conn.execute(
                "INSERT OR REPLACE INTO semantic_knowledge_relation
                (source_session_id,item_title,candidate_knowledge_id,relation,score,embedding_model,updated_at)
                VALUES (?1,?2,?3,?4,?5,?6,?7)",
                params![session_id,result.items[idx].title,id,decision,score,self.model,chrono::Utc::now().to_rfc3339()]
            ) { warn!(error=%e,"[DEDUP] relation save failed"); }
        }
    }

    async fn judge(&self, a:&V3KnowledgeItem,b:&V3KnowledgeItem)-> anyhow::Result<&'static str> {
        let prompt=format!(
            "判断两条工程知识的关系。必须对比故障现象、根因、解决方案和适用环境。即使标题相似，根因不同也不是相同知识。只能输出一个英文单词：same、related、different。\nA: {}\nB: {}",
            serde_json::to_string(a)?,serde_json::to_string(b)?
        );
        let answer=self.llm.chat_with_max_tokens(
            "你是严格的工程知识去重审核器。不要猜测未给出的事实。", &prompt, 16
        ).await?;
        let normalized=answer.trim().to_ascii_lowercase();
        Ok(match normalized.as_str() {
            "same"=>"same","related"=>"related",_=>"different"
        })
    }
}

fn knowledge_text(item:&V3KnowledgeItem)->String {
    format!("{}\n{}\n{}\n{}",item.title,item.summary,
        item.problem.as_deref().unwrap_or(""),item.content.chars().take(2400).collect::<String>())
}
fn merge_item(target:&mut V3KnowledgeItem,other:&V3KnowledgeItem) {
    if !target.content.contains(other.content.trim()) {
        target.content.push_str("\n\n---\n\n");
        target.content.push_str(&other.content);
    }
    for tag in &other.tags {
        if !target.tags.contains(tag) { target.tags.push(tag.clone()); }
    }
    target.confidence=target.confidence.max(other.confidence);
}
