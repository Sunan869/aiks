# AIKS V4.2 Backend Unification Design

## Status
Approved implementation baseline for the V4.2 post-UI backend work.

## Goal
Converge AIKS V4.2 onto one model layer, one embedding layer, one document indexing lifecycle, and one product-level unified search while keeping SiYuan as the canonical content/editor system.

## Ownership Boundary

### SiYuan owns
- canonical document body and title
- document tree and paths
- blocks, references, backlinks, outline, attributes and Attribute View
- graph and editor history
- editor-native/document-local search
- direct create/edit/delete operations performed in the Workbench

### AIKS owns
- LLM, embedding and optional reranking providers
- knowledge extraction and manual-knowledge AI assist
- read models, FTS, chunks, vector records and search ranking
- RAG/Agent retrieval
- session/knowledge mappings, provenance, hashes and index state
- pipeline orchestration and retry/error state

SiYuan must not expose or maintain a second AI/embedding configuration in the embedded profile.

## Model Service
Replace the current conceptual split between `ai`, legacy `extractor`, and `embedding` with one AIKS model capability layer:

```text
ModelService
├─ LLM
├─ Embedding
└─ Reranker (optional later)
```

LLM and embedding may use different physical models. “Unified” means all callers resolve capabilities through one service/registry rather than each feature owning its own endpoint/model configuration.

Legacy `ExtractorConfig` is migrated/deprecated; extraction uses the LLM capability.

## Unified Knowledge Index Lifecycle
The primary indexing unit is a canonical SiYuan knowledge document, not a source session.

```text
SiYuan document
  -> read canonical markdown
  -> compute content hash
  -> update AIKS read model
  -> FTS
  -> chunk
  -> embedding
  -> vector records
  -> READY
```

The operation is idempotent. If the indexed hash and embedding model identity match, no re-embedding is required.

Index state is tracked per knowledge item:
- `pending`
- `indexing`
- `ready`
- `stale`
- `failed`

Tracked metadata includes indexed hash/time, embedding model/dimensions, chunk count and last error.

### Document events
- `documentCreated`: if under `/20 Knowledge`, establish an AIKS knowledge identity and enqueue indexing.
- `documentChanged`: mark stale and enqueue full read-model/FTS/vector rebuild.
- `documentDeleted`: remove or soft-delete the read model and all derived search artifacts so it disappears from search immediately.

The session extraction pipeline may create knowledge, but it must call the same document-level index service after publication.

## Session Indexing
Raw AI sessions remain separate from canonical knowledge but are searchable semantically. They receive their own chunks/embeddings and are included in Unified Search. Knowledge and session indexes use the same embedding capability unless explicitly migrated to another configured model.

## Unified Search
The AIKS product-level search entry (“搜索知识和 AI 对话记录”) must call AIKS Unified Search, not `showWorkbenchSearch()`.

Unified Search covers at least:
- Knowledge
- AI Sessions

Retrieval pipeline:

```text
query
 ├─ lexical recall (FTS/title/summary/tags/content)
 ├─ semantic recall (query embedding + vector similarity)
 └─ metadata filters
      -> candidate fusion (RRF or equivalent)
      -> optional reranker later
      -> ranked results
```

SiYuan native search is retained for editor-native/block/document-local workflows, but is not the AIKS product search.

### Chinese query handling
Do not rely on whitespace splitting. Query analysis must preserve model names/error codes/paths while producing useful CJK lexical terms. Initial implementation may use pragmatic CJK token/bigram normalization; later versions may use a dedicated tokenizer.

### Vector implementation scope
V4.2 does not require Qdrant/Milvus. The first implementation should provide correct full/local recall for desktop-sized datasets and isolate the vector-store interface so sqlite-vec/HNSW can replace brute-force storage later.

## Manual Knowledge AI Assist
Manual knowledge is not “extraction”. It receives explicit AI Assist operations:
- summary
- tags
- category
- title suggestion
- key conclusions
- optional structure/rewrite suggestion

AI must not silently overwrite user-authored body content. Suggestions are previewed and applied explicitly.

## Bridge Boundary
The bridge is only for cross-system coordination.

Keep/carry:
- `bridgeReady`
- `openDocument`
- `openBlock`
- `openSession`
- readonly mode
- `documentCreated`
- `documentChanged`
- `documentDeleted`
- `requestAiAssist`
- cross-app navigation requests

Pure Workbench actions such as native search/graph/outline/database presentation should be owned directly by `aiks-siyuan`, not routed through AIKS merely to call back into SiYuan.

## Repository Responsibilities

### `Sunan869/aiks`
- ModelService / capability configuration
- document index service and index state
- session index service
- Unified Search
- AI Assist backend
- Tauri commands/events for search and assist
- bridge event consumption
- tests and migrations

### `Sunan869/aiks-siyuan`
- emit reliable canonical document lifecycle events
- call AIKS product search entry instead of using native search for the product-level search box
- emit AI Assist requests and display/apply returned suggestions
- keep native editor/graph/database/outline behavior local
- preserve current UI redesign

## Required End State
The system must not have separate product AI/embedding stacks or different indexing rules based on how a knowledge document was created.

```text
one Model Service
one Embedding Service
one Knowledge Index Service
one Unified Search

all Knowledge + all AI Sessions
 -> lexical + vector + metadata
```

SiYuan remains the canonical content/editor layer; AIKS remains the intelligence and control layer.
