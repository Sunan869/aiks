from pathlib import Path

path = Path("crates/aiks-core/src/indexing/session.rs")
text = path.read_text()

old_call = '''        self.finish_rebuild(
            input.session_id,
            &content_hash,
            &prepared,
            final_embedding_model,
            actual_dimensions,
            &vectors,
            embedding_error.as_deref(),
        )?;'''
new_call = '''        let finish = SessionFinishContext {
            session_id: input.session_id,
            content_hash: &content_hash,
            chunks: &prepared,
            embedding_model: final_embedding_model,
            dimensions: actual_dimensions,
            vectors: &vectors,
            last_error: embedding_error.as_deref(),
        };
        self.finish_rebuild(&finish)?;'''
if old_call not in text:
    raise SystemExit("finish_rebuild call target not found")
text = text.replace(old_call, new_call, 1)

old_fn = '''    fn finish_rebuild(
        &self,
        session_id: i64,
        content_hash: &str,
        chunks: &[PreparedSessionChunk],
        embedding_model: Option<&str>,
        dimensions: Option<usize>,
        vectors: &[Vec<f32>],
        last_error: Option<&str>,
    ) -> anyhow::Result<()> {
        if embedding_model.is_some() && chunks.len() != vectors.len() {'''
new_fn = '''    fn finish_rebuild(&self, finish: &SessionFinishContext<'_>) -> anyhow::Result<()> {
        let session_id = finish.session_id;
        let content_hash = finish.content_hash;
        let chunks = finish.chunks;
        let embedding_model = finish.embedding_model;
        let dimensions = finish.dimensions;
        let vectors = finish.vectors;
        let last_error = finish.last_error;

        if embedding_model.is_some() && chunks.len() != vectors.len() {'''
if old_fn not in text:
    raise SystemExit("finish_rebuild signature target not found")
text = text.replace(old_fn, new_fn, 1)

anchor = '''struct SessionRebuildContext<'a> {
    session_id: i64,
    external_id: &'a str,
    source: &'a str,
    title: &'a str,
    project_name: Option<&'a str>,
    normalized_text: &'a str,
    content_hash: &'a str,
    chunks: &'a [PreparedSessionChunk],
}
'''
addition = anchor + '''\n#[derive(Debug)]
struct SessionFinishContext<'a> {
    session_id: i64,
    content_hash: &'a str,
    chunks: &'a [PreparedSessionChunk],
    embedding_model: Option<&'a str>,
    dimensions: Option<usize>,
    vectors: &'a [Vec<f32>],
    last_error: Option<&'a str>,
}
'''
if anchor not in text:
    raise SystemExit("SessionRebuildContext anchor not found")
text = text.replace(anchor, addition, 1)
path.write_text(text)
