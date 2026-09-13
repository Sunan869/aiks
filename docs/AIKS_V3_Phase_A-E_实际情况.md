# AIKS V3 Phase A-E 改造实际情况与后续计划

版本：2026-09-13（更新）  
基线：V3 Alpha（commit 642d377）  
本次完成：Phase A + B + C + D + E  
验收结果：**构建成功** — `AIKS_0.2.0_x64-setup.exe (70 MB)`  
测试：84 个 Rust 单元测试全部通过  

---

## 一、Phase A-E 实际完成情况

### Phase A：Pipeline 自动触发（完成）

#### 新增模块

| 文件 | 说明 |
|------|------|
| `crates/aiks-core/src/pipeline/worker.rs` | 背景 Tokio worker，通过 mpsc channel 接收 PipelineJob |
| `crates/aiks-core/src/pipeline/session_chunker.rs` | LLM 分块（20k tokens / 40 消息），写入 session_chunk 表 |
| `crates/aiks-core/src/pipeline/cleaner.rs`（已有）| 清洗集成到 pipeline 流程 |

#### 关键变更

- `AiksEngine` 新增 `pipeline_worker: Arc<PipelineWorker>` 字段，引擎启动时自动启动 worker
- `sync_and_enqueue_extraction` 完成同步后，自动查询新增/更新 Session 的 DB ID 并提交 PipelineJob
- `enqueue_pipeline_for_session()` 供 Tauri 命令手动触发
- Pipeline 流程：`DISCOVERED → PARSED → CLEANED → LLM_CHUNKED → AI_EXTRACTED → EMBED_CHUNKED → EMBEDDED → INDEXED → READY`

---

### Phase B：AI Extraction V3（完成）

#### 新增模块

| 文件 | 说明 |
|------|------|
| `crates/aiks-core/src/ai/schema_v3.rs` | V3ExtractionResult（items[]数组）、V3KnowledgeItem |
| `crates/aiks-core/src/ai/prompts_v3.rs` | SYSTEM_PROMPT_V3、make_v3_extraction_prompt、make_v3_final_prompt |
| `crates/aiks-core/src/pipeline/ai_stage.rs` | AiStage：调用 AI，解析 JSON，写入 knowledge_item 表 |
| `crates/aiks-core/src/pipeline/knowledge_repo.rs` | KnowledgeRepo：CRUD + FTS5 索引维护 |

#### 核心升级

- V2.5: 1 Session → 1 Knowledge（`KnowledgeDocument`）
- V3: 1 Session → 0~N KnowledgeItems（`V3ExtractionResult.items[]`）
- Map-Reduce：长 Session → 分块摘要 → 汇总提炼
- `ai_request_log` 记录每次 AI 调用（model、endpoint、tokens、latency）
- `knowledge_fts` 虚拟表同步更新（FTS5 全文检索）

---

### Phase C：Embedding + 向量搜索（完成）

#### 新增模块

| 文件 | 说明 |
|------|------|
| `crates/aiks-core/src/pipeline/embedding_client.rs` | EmbeddingClient + EmbeddingConfig（OpenAI-compatible API），cosine_sim() |
| `crates/aiks-core/src/pipeline/embedding_stage.rs` | 两个子阶段：EMBED_CHUNK（知识拆片）+ EMBED（调用 API 存 BLOB） |
| `crates/aiks-core/src/pipeline/search.rs` | hybrid_search()：FTS5 + 向量余弦（0.35/0.65 加权） |

#### 关键设计

- **不引入 sqlite-vec**：向量存为 BLOB（float32 小端序），Rust 侧计算余弦相似度
- 好处：零额外依赖，适合单 EXE 产品
- Embedding 默认禁用（`enabled: false`），UI 明确显示"未配置"
- 配置后自动启用：chunk → embed → in-memory cosine search
- `EmbeddingConfig` 添加到 `Config` struct，支持 TOML 配置

---

### Phase D：处理中心详情 UI（完成）

#### 新增页面

| 文件 | 说明 |
|------|------|
| `apps/aiks-desktop/src/pages/ProcessingDetailPage.tsx` | 流水线详情：每阶段展开卡片（输入/输出/耗时/错误），重试按钮 |
| `apps/aiks-desktop/src/pages/SessionDetailPage.tsx` | 工作记录详情：Session 信息、LLM 切片列表、知识条目、管道链接 |

#### 新增 Tauri 命令

- `run_pipeline_for_session` — 手动触发处理（支持重试）
- `get_session_detail` — Session 详情 + chunks + knowledge

---

### Phase E：知识库详情 UI + 来源溯源（完成）

#### 新增页面

| 文件 | 说明 |
|------|------|
| `apps/aiks-desktop/src/pages/KnowledgeDetailPage.tsx` | 知识详情：标题/摘要/全文/标签/向量切片/来源链接 |

#### 新增 Tauri 命令

- `get_knowledge_detail` — 知识条目详情 + embedding chunks
- `hybrid_search` — 混合搜索（FTS5 + 向量）

#### 导航升级

- `App.tsx` 引入 `NavState`（无需 react-router），支持 detail 视图切换
- Sessions → SessionDetail → PipelineDetail
- KnowledgeList → KnowledgeDetail → SessionDetail
- ProcessingList → ProcessingDetail（retry）
- 所有列表页支持 `onViewDetail` callback

---

## 二、测试结果

```
cargo test -p aiks-core --lib --quiet

running 84 tests
....................................................................
test result: ok. 84 passed; 0 failed
```

**新增测试（Phase A-E）：**
- `pipeline_stage_roundtrip` — PipelineStage 枚举序列化
- `pipeline_stage_sequence` — next() 链路验证
- `knowledge_category_from_str` — 分类解析
- `clean_removes_empty` — 清洗：空消息
- `clean_preserves_error_tool_result` — 清洗：保留错误
- `chunking_splits_large_sessions` — 100 条消息 → 多 chunk
- `chunking_small_session_is_one_chunk` — 5 条消息 → 1 chunk
- `parse_valid_v3_response` — V3 JSON 解析
- `parse_skip_response` — 不值得提炼场景
- `parse_json_with_fences` — 代码块去除
- `cosine_identical_vectors` — 余弦：相同向量 = 1.0
- `cosine_orthogonal_vectors` — 余弦：正交向量 = 0.0
- `split_short_text_is_one_chunk` — 短文本 = 1 块
- `split_long_text_creates_chunks` — 长文本多块带 overlap

---

## 三、本地 OpenCode 数据验证

本地 OpenCode DB 位置：
```
C:\Users\Administrator\.local\share\opencode\opencode.db (1639 MB)
```

OpenCode Provider 测试（fixture）：3/3 通过  
实际 DB 扫描：Provider 正确找到真实 DB（1639 MB），可在应用启动后触发"扫描"读取真实 Sessions。

---

## 四、当前架构状态

```
AIKS V3 — Phase A-E 完成
│
├── 数据层 ──────────────────────────────────── ✅
│   ├── V1 表（保留）
│   └── V3 表（8张，含 FTS5 虚拟表）
│
├── Pipeline Worker ─────────────────────────── ✅
│   ├── Tokio 背景任务（mpsc channel）
│   ├── PARSE → CLEAN → CHUNK → AI → EMBED_CHUNK → EMBED → INDEX → READY
│   └── 单 Session 失败不影响其他
│
├── AI Extraction V3 ────────────────────────── ✅
│   ├── 1 Session → 0~N KnowledgeItems
│   ├── Map-Reduce（长 Session）
│   └── ai_request_log 可观察
│
├── Embedding ──────────────────────────────── ✅（配置后启用）
│   ├── OpenAI-compatible API
│   ├── BLOB 向量存储
│   └── 余弦相似度 in-process
│
├── Hybrid Search ───────────────────────────── ✅
│   ├── FTS5 全文
│   ├── 向量余弦（可选）
│   └── 0.35/0.65 加权合并
│
├── Tauri 命令层 ────────────────────────────── ✅（18 个命令）
│
└── 前端页面 ────────────────────────────────── ✅
    ├── SessionsPage → SessionDetailPage
    ├── KnowledgeBasePageV3 → KnowledgeDetailPage
    ├── ProcessingPage → ProcessingDetailPage（retry）
    ├── SearchPage（hybrid search）
    └── Mock API（VITE_AIKS_MOCK=true）
```

---

## 五、未完成 / 后续计划

### 已知尚未接通的部分

| 项目 | 现状 | 说明 |
|------|------|------|
| SiYuan 弱化 | 仍为主 Knowledge UI | V3 Phase F 计划 |
| AI 提炼真实 E2E | AI 调用依赖公司内网 | 需连接 `http://10.10.23.16:18000/v1` |
| Embedding 真实 E2E | 未配置 Embedding 模型 | 需接入 vLLM Embedding |
| CLI V3 子命令 | 未实现 `aiks pipeline run <id>` 等 | 后续迭代 |
| 分页搜索结果 | 搜索结果限 20 条 | 可扩展 |

### Phase F 计划：SiYuan 弱化（1天）

1. 将 SiYuan UI 入口移至「设置 → 高级」
2. 在 KnowledgeDetailPage 增加「导出到 SiYuan」按钮
3. 默认不启动 SiYuan Runtime（保留 config 选项）

### Phase G 计划：版本号升级到 0.3.0

1. 更新 `Cargo.toml` workspace version
2. 更新 CHANGELOG
3. 正式命名为 V3.0-beta

---

## 六、开发模式

### Mock 前端（秒启动）
```powershell
cd apps\aiks-desktop
$env:VITE_AIKS_MOCK="true"; npm run dev
# http://localhost:1420
```

### Tauri 开发模式（HMR）
```powershell
.\scripts\dev.ps1
```

### Release 打包
```powershell
.\scripts\build-release.ps1
# 输出：target\release\bundle\nsis\AIKS_0.2.0_x64-setup.exe (70 MB)
```

---

## 七、Embedding 配置示例（config.toml）

```toml
[embedding]
enabled = true
base_url = "http://10.10.23.16:18000/v1"
model = "bge-m3"
dimensions = 1024
batch_size = 16
chunk_target_tokens = 800
chunk_max_tokens = 1200
chunk_overlap_tokens = 120
```

---

## 八、关键文件索引（Phase A-E 新增）

| 文件 | 说明 |
|------|------|
| `crates/aiks-core/src/pipeline/worker.rs` | V3 Pipeline Worker |
| `crates/aiks-core/src/pipeline/session_chunker.rs` | LLM 分块 |
| `crates/aiks-core/src/pipeline/ai_stage.rs` | V3 AI 提炼阶段 |
| `crates/aiks-core/src/pipeline/knowledge_repo.rs` | Knowledge CRUD |
| `crates/aiks-core/src/pipeline/embedding_client.rs` | Embedding API + cosine_sim |
| `crates/aiks-core/src/pipeline/embedding_stage.rs` | Embedding 两个子阶段 |
| `crates/aiks-core/src/pipeline/search.rs` | Hybrid Search |
| `crates/aiks-core/src/ai/schema_v3.rs` | V3ExtractionResult |
| `crates/aiks-core/src/ai/prompts_v3.rs` | V3 Prompts |
| `apps/aiks-desktop/src/pages/SessionDetailPage.tsx` | 工作记录详情 |
| `apps/aiks-desktop/src/pages/KnowledgeDetailPage.tsx` | 知识详情 |
| `apps/aiks-desktop/src/pages/ProcessingDetailPage.tsx` | 处理详情 |
