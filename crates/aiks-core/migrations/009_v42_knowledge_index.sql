UPDATE knowledge_item
SET index_status = 'pending'
WHERE index_status IS NULL OR TRIM(index_status) = '';

UPDATE knowledge_item
SET index_chunk_count = 0
WHERE index_chunk_count IS NULL;

CREATE INDEX IF NOT EXISTS idx_knowledge_item_index_status
ON knowledge_item(index_status);

CREATE INDEX IF NOT EXISTS idx_knowledge_item_siyuan_index_status
ON knowledge_item(siyuan_doc_id, index_status);
