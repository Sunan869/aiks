from pathlib import Path

search_path = Path("crates/aiks-core/src/search/mod.rs")
text = search_path.read_text()
old = '''pub struct UnifiedSearchService {
    db: Arc<StateDb>,
    embeddings: Arc<dyn EmbeddingProvider>,
}

impl UnifiedSearchService {
    pub fn new(db: Arc<StateDb>, embeddings: Arc<dyn EmbeddingProvider>) -> Self {
        Self { db, embeddings }
    }
'''
new = '''enum SearchDb<'a> {
    Owned(Arc<StateDb>),
    Borrowed(&'a StateDb),
}

impl std::ops::Deref for SearchDb<'_> {
    type Target = StateDb;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Owned(db) => db.as_ref(),
            Self::Borrowed(db) => db,
        }
    }
}

pub struct UnifiedSearchService<'a> {
    db: SearchDb<'a>,
    embeddings: Arc<dyn EmbeddingProvider>,
}

impl UnifiedSearchService<'static> {
    pub fn new(db: Arc<StateDb>, embeddings: Arc<dyn EmbeddingProvider>) -> Self {
        Self {
            db: SearchDb::Owned(db),
            embeddings,
        }
    }
}

impl<'a> UnifiedSearchService<'a> {
    pub fn borrowed(db: &'a StateDb, embeddings: Arc<dyn EmbeddingProvider>) -> Self {
        Self {
            db: SearchDb::Borrowed(db),
            embeddings,
        }
    }
'''
if old not in text:
    raise SystemExit("UnifiedSearchService constructor block not found")
search_path.write_text(text.replace(old, new, 1))

legacy_path = Path("crates/aiks-core/src/pipeline/search.rs")
text = legacy_path.read_text()
if text.count("db: &Arc<StateDb>,") != 2:
    raise SystemExit("expected exactly two Arc DB signatures in legacy search")
text = text.replace("db: &Arc<StateDb>,", "db: &StateDb,")
old_call = "UnifiedSearchService::new(Arc::clone(db), embeddings)"
if old_call not in text:
    raise SystemExit("legacy unified search constructor call not found")
text = text.replace(old_call, "UnifiedSearchService::borrowed(db, embeddings)", 1)
legacy_path.write_text(text)
