"""Exact Task 6 Core wiring. No ref updates; prepare job emits reviewable blobs."""
from pathlib import Path
import subprocess
R = Path('crates/aiks-core/src')
def load(name, expected):
    path=R/name
    actual=subprocess.check_output(['git','hash-object',str(path)],text=True).strip()
    assert actual==expected,(name,actual,expected)
    return path,path.read_text()
def sub(text,old,new,count=1):
    assert text.count(old)==count,(old[:90],text.count(old),count)
    return text.replace(old,new)

p,s=load('service/mod.rs','ed2146d7ee2e6c51b5db2f4178139bc70c1ff651')
s += '\npub mod query;\nmod runtime;\npub use runtime::{ServiceRuntime, ServiceRuntimeConfig};\n'
p.write_text(s)
p,s=load('service/ingestion.rs','2e7d7cb82661158f4e0aab3677fb865983ae1285')
old='''    pub fn accept(
        &self,
        context: &LocalContext,
        snapshot: &ValidatedSnapshot,
    ) -> Result<(SnapshotReceipt, bool), ServiceError> {'''
s=sub(s,old,old+'''
        let (receipt, queued, _) = self.accept_with_flags(context, snapshot)?;
        Ok((receipt, queued))
    }

    pub(crate) fn accept_with_flags(
        &self,
        context: &LocalContext,
        snapshot: &ValidatedSnapshot,
    ) -> Result<(SnapshotReceipt, bool, bool), ServiceError> {''')
assert s.count('return Ok((receipt, false));')==2
s=s.replace('return Ok((receipt, false));','return Ok((receipt, false, false));',1)
s=s.replace('return Ok((receipt, false));','return Ok((receipt, false, true));',1)
s=sub(s,'Ok((receipt, queued.inserted))','Ok((receipt, queued.inserted, true))')
p.write_text(s)
p,s=load('storage/db.rs','6cfeb88ae7dc27ece0ca59f7dd4c5dc3b9a69bac')
s=sub(s,'        tx.commit()?;\n        Ok(())\n    }\n\n    fn ensure_column(','''        tx.execute_batch(include_str!("../../migrations/014_service_read_epoch.sql"))
            .context("run V15 service read epoch migration")?;
        tx.commit()?;
        Ok(())
    }

    fn ensure_column(''')
p.write_text(s)
p,s=load('search/mod.rs','209a10ead83e0f1fbf2b16265330b6c16da54d31')
s=sub(s,'mod lexical;','mod lexical;\nmod scope;\nuse scope::ScopedFilter;')
s=sub(s,"pub struct UnifiedSearchService<'a> {\n    db: SearchDb<'a>,\n    embeddings: Arc<dyn EmbeddingProvider>,\n}","pub struct UnifiedSearchService<'a> {\n    db: SearchDb<'a>,\n    embeddings: Arc<dyn EmbeddingProvider>,\n    context: Option<crate::service::LocalContext>,\n}")
s=sub(s,'            embeddings,\n        }','            embeddings,\n            context: None,\n        }',2)
s=sub(s,"impl UnifiedSearchService<'static> {",'''impl UnifiedSearchService<'static> {
    pub(crate) fn scoped(db: Arc<StateDb>, embeddings: Arc<dyn EmbeddingProvider>, context: crate::service::LocalContext) -> Self {
        Self { db: SearchDb::Owned(db), embeddings, context: Some(context) }
    }
''')
s=sub(s,'        let started = Instant::now();\n        let query = query.trim();','        let filter = ScopedFilter { filter, context: self.context.clone() };\n        let started = Instant::now();\n        let query = query.trim();')
s=s.replace('filter: &UnifiedSearchFilter','filter: &ScopedFilter')
for function,corpus in [('semantic_knowledge','Knowledge'),('semantic_sessions','Session')]:
    a=s.index('    fn '+function+'(')
    b=s.index('        let db_started',a)
    part=s[a:b]
    part=sub(part,'        let mut stmt = conn.prepare(','        let sql = format!(')
    part=sub(part,'             LIMIT ?2",\n        )?;','             AND {} LIMIT ?2",\n            filter.predicate(SearchCorpus::'+corpus+'),\n        );\n        let mut stmt = conn.prepare(&sql)?;')
    s=s[:a]+part+s[b:]
p.write_text(s)
p,s=load('search/lexical.rs','93b7b6ffba5c208a90020cb63872c6d8db6c59c1')
s=s.replace('UnifiedSearchFilter','ScopedFilter')
s=sub(s,'    let mut stmt = conn.prepare(sql)?;','    let sql = sql.replace("ORDER BY rank", &format!("AND {} ORDER BY rank", filter.predicate(corpus)));\n    let mut stmt = conn.prepare(&sql)?;')
s=sub(s,'    let sql = format!("{select} AND ({predicates}) LIMIT {CANDIDATE_CAP}");','    let sql = format!("{select} AND ({predicates}) AND {} LIMIT {CANDIDATE_CAP}", filter.predicate(corpus));')
p.write_text(s)
p,s=load('sink/siyuan.rs','25e201a6b0a914855ffbaf5238d9c5266ac4f281')
s=sub(s,'    pub fn sink_name()', '''    /// Read-only service adapter: no credential forwarding through redirects.
    pub fn service_reader(mut config: SiYuanConfig) -> anyhow::Result<Self> {
        let url = reqwest::Url::parse(&config.base_url)?;
        anyhow::ensure!(matches!(url.scheme(), "http" | "https") && url.username().is_empty()
            && url.password().is_none() && url.query().is_none() && url.fragment().is_none()
            && matches!(url.path(), "" | "/"), "Invalid content store origin");
        config.base_url = config.base_url.trim_end_matches('/').to_string();
        let mut sink = Self::new(config)?;
        sink.client = Client::builder().no_proxy().redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(10)).build()?;
        Ok(sink)
    }

    /// Fixed endpoint and trusted mapped document ID; never a caller URL/path.
    pub async fn get_document_markdown_bounded(&self, doc_id: &str, budget: usize) -> anyhow::Result<String> {
        anyhow::ensure!(!doc_id.is_empty() && doc_id.len()<=256, "Invalid mapped document identity");
        let url = format!("{}/api/block/getBlockKramdown",self.base_url);
        let mut response = self.request_builder(reqwest::Method::POST,&url)
            .json(&serde_json::json!({"id":doc_id})).send().await?;
        anyhow::ensure!(response.status().is_success(), "Content store unavailable");
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            anyhow::ensure!(bytes.len().saturating_add(chunk.len())<=budget, "Content response exceeds budget");
            bytes.extend_from_slice(&chunk);
        }
        #[derive(Deserialize)]
        struct Content { kramdown: String }
        let parsed: ApiResponse<Content> = serde_json::from_slice(&bytes)?;
        anyhow::ensure!(parsed.code==0, "Content store rejected mapped document");
        let content=parsed.data.context("Content store returned no document")?;
        Ok(strip_kramdown_attrs(&content.kramdown))
    }

    pub fn sink_name()''')
p.write_text(s)
print('Applied exact Task 6 Core wiring. Canonical tests remain required.')
