# AIKS V3 — Unified Knowledge Pipeline 统一知识处理平台改造方案

版本：V3.0 Proposal  
适用项目：AI Knowledge Sync / AIKS Desktop  
目标用户：公司内部研发人员、算法工程师、测试人员、技术管理人员  
目标定位：将当前“Session 同步器 + 外部知识库 UI”重构为“统一、可观察、可追溯、可调试的 AI 工作知识处理平台”

---

# 1. 背景与问题判断

当前 AIKS V2.5 已经具备较完整的数据采集、Session 解析、Embedded SiYuan、AI 提炼、桌面应用和安装包能力，但实际使用后暴露出一个更本质的问题：

> 产品的“技术组件已经很多”，但“用户体验和知识处理链路仍然割裂”。

当前实际使用体验接近：

```text
OpenCode / Codex / Gemini / Claude
              ↓
           AIKS 扫描
              ↓
           AIKS 同步
              ↓
        Embedded SiYuan
              ↓
      另一个知识库界面
              ↓
    后台 AI 提炼 / 搜索
```

用户会产生以下感受：

- “数据源同步”像一个独立模块
- “知识库管理”像另一个独立产品
- AIKS 与 SiYuan 的边界过于明显
- 看不到 Session 是如何被处理成 Knowledge 的
- 看不到切片过程
- 看不到 AI 总结过程
- 看不到 Embedding 过程
- 看不到向量索引过程
- 看不到检索命中的具体 Chunk
- 看不到原始 Session 与最终 Knowledge 的关系
- “同步完成”以后直接就变成“知识”，中间是黑盒
- UI 更像后台管理系统，而不是知识处理平台
- 调试页面需要频繁重新打包 EXE，研发效率极低

因此下一阶段不建议继续在当前 UI 上增加零散功能，而应该进行一次明确的产品与架构收口。

---

# 2. V3 核心目标

AIKS V3 的三个关键词：

```text
一体化
可观察
易调试
```

最终希望用户看到的是一个完整流程：

```text
AI 工具
  ↓
Session
  ↓
解析
  ↓
清洗
  ↓
LLM 切片
  ↓
AI 提炼
  ↓
Knowledge Item
  ↓
Embedding 切片
  ↓
Embedding
  ↓
Vector Index
  ↓
Search / Knowledge
```

而不是：

```text
同步
↓
知识
```

---

# 3. V3 产品定位调整

## 3.1 原定位

当前 AIKS 更接近：

> AI Session Collector + SiYuan Knowledge Sink

即：

```text
AIKS
负责采集与同步

SiYuan
负责知识管理与搜索
```

这种架构开发速度快，但会导致：

- AIKS 失去知识处理过程控制权
- Embedding / Vector Index 成为黑盒
- UI 无法展示中间处理状态
- 用户需要理解另一个完整知识库产品
- 产品体验割裂

## 3.2 新定位

V3 推荐正式定位：

> AIKS 是企业内部个人 AI 工作知识处理平台，负责从 AI 工具采集工作记录，并对 Session 进行清洗、切片、AI 提炼、向量化、索引、搜索和知识管理。

最终：

```text
AIKS
├── 数据采集
├── Session 管理
├── Processing Pipeline
├── AI Extraction
├── Embedding
├── Vector Index
├── Search
├── Knowledge UI
└── Optional Export
```

SiYuan 从“核心知识库 UI”调整为：

```text
Optional Sink / Export Target
```

即：

```text
AIKS Knowledge
      ↓
可选：
[导出到 SiYuan]
```

而不是：

```text
AIKS
↓
必须依赖 SiYuan 才能完成知识管理
```

---

# 4. 总体架构

推荐 V3 架构：

```text
┌──────────────────────────────────────────────────────┐
│                    AIKS Desktop                      │
│                                                      │
│  Sources                                             │
│  ├─ OpenCode                                         │
│  ├─ Codex                                            │
│  ├─ Gemini                                           │
│  └─ Claude                                           │
│          │                                           │
│          ▼                                           │
│  Session Ingestion                                   │
│          │                                           │
│          ▼                                           │
│  Normalization                                       │
│          │                                           │
│          ▼                                           │
│  Processing Pipeline                                 │
│  ├─ Clean                                            │
│  ├─ LLM Chunk                                        │
│  ├─ AI Extraction                                    │
│  ├─ Knowledge Units                                  │
│  ├─ Embedding Chunk                                  │
│  ├─ Embedding                                        │
│  └─ Vector Index                                     │
│          │                                           │
│          ▼                                           │
│  Unified Knowledge Store                             │
│          │                                           │
│          ├─ Search                                   │
│          ├─ Project View                             │
│          ├─ Timeline                                 │
│          ├─ Knowledge                                │
│          └─ Raw Session Trace                        │
│                                                      │
└──────────────────────────────────────────────────────┘
                │
                └──── Optional Export
                      └─ SiYuan
```

---

# 5. 核心数据链路

V3 必须明确区分以下对象。

## 5.1 SourceSession

表示源工具中的原始 Session。

```rust
pub struct SourceSession {
    pub source: SourceKind,
    pub external_session_id: String,
    pub source_path: Option<String>,
    pub project_name: Option<String>,
    pub project_path: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub metadata: Value,
}
```

它只代表：

```text
“源系统里有一条 Session”
```

## 5.2 NormalizedSession

Provider 解析后得到统一格式：

```rust
pub struct NormalizedSession {
    pub source: SourceKind,
    pub external_session_id: String,
    pub title: Option<String>,
    pub project_name: Option<String>,
    pub project_path: Option<String>,
    pub messages: Vec<NormalizedMessage>,
    pub metadata: Value,
}
```

## 5.3 SessionChunk

用于 LLM 分块处理。

```rust
pub struct SessionChunk {
    pub id: String,
    pub session_id: String,
    pub chunk_index: i32,
    pub message_start: i32,
    pub message_end: i32,
    pub token_count: i32,
    pub content: String,
    pub content_hash: String,
}
```

用途：

```text
长 Session
↓
LLM Summarization
```

## 5.4 KnowledgeItem

AI 提炼出来的知识单元。

V3 不再限定：

```text
1 Session = 1 Knowledge
```

允许：

```text
1 Session = 0~N KnowledgeItems
```

例如一个长 OpenCode Session 中：

```text
知识点 1：PowerShell Parser Bug
知识点 2：SiYuan NSIS Runtime 提取
知识点 3：Tauri NativeCommandError
知识点 4：升级安装文件锁
```

定义：

```rust
pub struct KnowledgeItem {
    pub id: String,
    pub source_session_id: String,
    pub project_name: Option<String>,
    pub title: String,
    pub category: KnowledgeCategory,
    pub summary: String,
    pub content: String,
    pub tags: Vec<String>,
    pub confidence: f32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

## 5.5 KnowledgeChunk

用于搜索和 Embedding。

与 SessionChunk 完全不同。

```rust
pub struct KnowledgeChunk {
    pub id: String,
    pub knowledge_id: String,
    pub heading: Option<String>,
    pub chunk_index: i32,
    pub token_count: i32,
    pub text: String,
    pub content_hash: String,
}
```

## 5.6 EmbeddingRecord

```rust
pub struct EmbeddingRecord {
    pub chunk_id: String,
    pub model: String,
    pub dimensions: i32,
    pub vector: Vec<f32>,
    pub created_at: DateTime<Utc>,
}
```

---

# 6. 两种 Chunk 必须完全分开

这是 V3 的关键设计。

## 6.1 LLM Chunk

作用：

```text
解决 Session 太长，超过 AI 模型 Context
```

例如：

```text
140 Messages
↓
Chunk 1：1-30
Chunk 2：31-60
Chunk 3：61-90
Chunk 4：91-120
Chunk 5：121-140
```

用于：

```text
Qwen3.8-27B
```

推荐第一版：

```text
20,000 ~ 30,000 tokens / chunk
或者
30 ~ 50 messages / chunk
```

取两者最先达到的限制。

## 6.2 Embedding Chunk

作用：

```text
用于向量检索 / RAG
```

应该更细。

例如：

```text
Knowledge Item
↓
按 Heading / Paragraph / Code Block
↓
500 ~ 1200 tokens/chunk
```

推荐：

```text
target: 800 tokens
max:    1200 tokens
overlap: 100~150 tokens
```

## 6.3 禁止混用

错误：

```text
LLM Chunk
直接作为 Embedding Chunk
```

因为：

- LLM Chunk 太大
- 搜索召回粒度粗
- 相似度不准确
- Retrieval Context 太长

---

# 7. Processing Pipeline

新增核心模块：

```text
pipeline/
├── mod.rs
├── orchestrator.rs
├── cleaner.rs
├── session_chunker.rs
├── ai_extract_stage.rs
├── knowledge_splitter.rs
├── embedding_chunker.rs
├── embedding_stage.rs
├── index_stage.rs
└── status.rs
```

统一流程：

```text
DISCOVERED
↓
PARSED
↓
NORMALIZED
↓
CLEANED
↓
LLM_CHUNKED
↓
AI_EXTRACTED
↓
KNOWLEDGE_SPLIT
↓
EMBED_CHUNKED
↓
EMBEDDED
↓
INDEXED
↓
READY
```

---

# 8. 每个阶段都必须有状态

不能再只有：

```text
synced=true
```

新增：

```rust
pub enum PipelineStage {
    Discovered,
    Parsed,
    Normalized,
    Cleaned,
    LlmChunked,
    AiExtracted,
    KnowledgeSplit,
    EmbedChunked,
    Embedded,
    Indexed,
    Ready,
    Failed,
}
```

---

# 9. Pipeline Run

每次处理必须记录：

```sql
CREATE TABLE pipeline_run (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    status TEXT NOT NULL,
    current_stage TEXT,
    started_at TEXT,
    finished_at TEXT,
    source_hash TEXT,
    pipeline_version TEXT,
    error_stage TEXT,
    error_message TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
```

---

# 10. Stage Run

进一步记录每个阶段：

```sql
CREATE TABLE pipeline_stage_run (
    id TEXT PRIMARY KEY,
    pipeline_run_id TEXT NOT NULL,
    stage TEXT NOT NULL,
    status TEXT NOT NULL,
    started_at TEXT,
    finished_at TEXT,
    input_count INTEGER,
    output_count INTEGER,
    latency_ms INTEGER,
    detail_json TEXT,
    error_message TEXT
);
```

这样 UI 才能展示：

```text
Parse      128 ms
Clean       32 ms
Chunk       21 ms
AI        18.4 s
Embedding  1.2 s
Index      180 ms
```

---

# 11. Session Processing Detail 页面

必须新增。

例如：

```text
项目骨架实现指导
OpenCode
ses_f717

状态：
已完成

────────────────────

原始消息

140 条

[查看原始会话]

────────────────────

解析

✓ 成功
140 messages
98 ms

────────────────────

清洗

✓ 成功

输入：
140

输出：
126

移除：
14

[查看清洗结果]

────────────────────

LLM 切片

✓ 成功

6 chunks

Chunk #1
Messages 1~24
18,230 tokens

Chunk #2
Messages 25~47
21,883 tokens

...

[查看全部 Chunk]

────────────────────

AI 提炼

✓ 成功

模型：
Qwen3.8-27B

耗时：
18.4s

输入：
6 Chunk Summaries

输出：
4 Knowledge Items

[查看模型 JSON]

────────────────────

Embedding Chunk

✓ 成功

Knowledge #1
4 chunks

Knowledge #2
3 chunks

总计：
11 chunks

────────────────────

Embedding

✓ 11 / 11

Model:
xxx-embedding

Dimensions:
1024

耗时：
1.4s

────────────────────

Vector Index

✓ Indexed

11 vectors

────────────────────

最终知识

4 items

[查看知识]
```

---

# 12. Processing Center 页面

新增一级菜单：

```text
处理中心
```

主界面：

```text
处理中心

全部         660
处理中        12
失败           3
已完成       645

Session                   Parse Clean Chunk AI Embed Index
──────────────────────────────────────────────────────────
项目骨架实现指导            ✓     ✓     ✓    ✓   ✓     ✓
SiYuan Runtime 修复         ✓     ✓     ✓    ✓   ✓     ✓
EF Core 查询                ✓     ✓     ✓    …   -     -
Docker 构建失败             ✓     ✓     ✕    -   -     -
```

---

# 13. 状态颜色

建议：

```text
✓ 绿色：成功
… 蓝色：处理中
! 黄色：等待
✕ 红色：失败
- 灰色：尚未执行
```

不要使用过多彩色 Card。

---

# 14. 数据库结构建议

统一使用一个嵌入式 SQLite：

```text
%LOCALAPPDATA%\AIKnowledgeSync\data\aiks.db
```

建议表：

```text
source_session
message
session_chunk
pipeline_run
pipeline_stage_run
knowledge_item
knowledge_chunk
embedding_record
sync_run
source_file_state
ai_request_log
search_history
```

---

# 15. 本地全文搜索

建议：

```text
SQLite FTS5
```

建立：

```text
knowledge_fts
session_fts
```

搜索范围：

```text
Knowledge Title
Knowledge Content
Raw Session
Project
Tag
```

---

# 16. Vector Store

桌面端优先：

```text
sqlite-vec
```

而不是：

```text
Qdrant
Milvus
Weaviate
Elastic
```

原因：

- 无额外服务
- 安装简单
- 本地数据
- 单 EXE 产品体验
- 方便备份
- 适合个人知识规模

---

# 17. Embedding Model

V3 需要明确引入真正的 Embedding Model。

当前：

```text
Qwen3.8-27B
```

用于：

```text
Summarization / Extraction
```

不建议直接拿生成模型当 Embedding Model。

推荐配置：

```rust
pub struct EmbeddingConfig {
    pub enabled: bool,
    pub base_url: String,
    pub model: String,
    pub dimensions: Option<usize>,
    pub batch_size: usize,
}
```

---

# 18. Embedding API

支持 OpenAI-compatible：

```http
POST /v1/embeddings
```

请求：

```json
{
  "model": "embedding-model-name",
  "input": [
    "chunk text 1",
    "chunk text 2"
  ]
}
```

---

# 19. Embedding Model 未确定时的策略

如果当前内部 vLLM 还没有 Embedding 模型：

V3 第一阶段允许：

```text
Embedding Stage = DISABLED
```

UI 明确显示：

```text
Embedding
未配置

Vector Index
未启用
```

不能伪装成：

```text
已完成
```

---

# 20. AI Extraction 模型

继续使用：

```text
Base URL:
http://127.0.0.1:11434/v1

Model:
Qwen3.8-27B
```

用于：

```text
Chunk Summary
Knowledge Extraction
Classification
Tagging
Knowledge Unit Split
```

---

# 21. AI Extraction 输出改成 0~N Knowledge Items

当前一 Session 一 Knowledge 的设计需要升级。

推荐 JSON：

```json
{
  "session_summary": "本次会话主要解决 AIKS V2.5 的多个构建和集成问题。",
  "knowledge_score": 0.92,
  "items": [
    {
      "title": "PowerShell 版本文件解析错误",
      "category": "troubleshooting",
      "summary": "使用 -contains 判断字符串包含关系导致配置解析失败。",
      "problem": "...",
      "root_causes": ["..."],
      "solutions": ["..."],
      "key_commands": ["..."],
      "key_files": ["..."],
      "tags": ["PowerShell", "AIKS"]
    },
    {
      "title": "Windows PowerShell NativeCommandError",
      "category": "troubleshooting",
      "summary": "...",
      "problem": "...",
      "root_causes": ["..."],
      "solutions": ["..."],
      "tags": ["PowerShell", "Cargo"]
    }
  ]
}
```

---

# 22. Cleaner

新增 Session Cleaner。

不能直接把原始 Session 全量喂模型。

处理：

```text
重复日志
巨大 stdout
重复 tool output
重复 assistant explanation
空 Message
无意义 Step Start/Finish
```

但必须保留：

```text
错误信息
命令
路径
版本
API
代码
配置
最终结果
```

---

# 23. Cleaner 输出必须可追踪

UI 显示：

```text
原始消息：140
清洗后：126
删除：14
```

并支持：

```text
[查看被过滤内容]
```

这样不会形成黑盒。

---

# 24. AI Request Trace

模型调用也必须可观察。

记录：

```text
Model
Endpoint
Prompt Version
Input Tokens
Output Tokens
Latency
Status
Retry Count
```

建议表：

```text
ai_request_log
```

---

# 25. Embedding 可视化

处理详情：

```text
Embedding

Model:
bge-m3

Chunks:
11

Succeeded:
11

Failed:
0

Dimension:
1024

Latency:
1.2s
```

---

# 26. Vector Index 可视化

显示：

```text
Index Status:
Indexed

Vectors:
11

Index Type:
sqlite-vec

Updated:
11:32:41
```

---

# 27. Search Trace

全局搜索增加：

```text
搜索详情
```

开发者模式可展开：

```text
Query:
"SiYuan Runtime"

Keyword Search:
FTS5
12 hits

Vector Search:
topK=20
20 hits

Rerank:
10 results

Final:
10
```

---

# 28. Hybrid Search

推荐：

```text
FTS5
+
Vector Search
```

第一版简单合并：

```text
final_score =
0.35 * keyword_score
+
0.65 * vector_score
```

后续再引入 reranker。

---

# 29. Knowledge 页面重新设计

Knowledge 不再是：

```text
打开 SiYuan
```

而是 AIKS 自己的核心页面。

推荐：

```text
知识库

全部
项目
类型
标签
收藏

────────────────

AIKS Runtime 集成修复
故障排查
AIKS
2026-09-12

PowerShell 构建链路修复
故障排查
AIKS
2026-09-12
```

---

# 30. Knowledge Detail

```text
标题

摘要

问题

根因

解决方案

关键命令

关键文件

设计决策

标签

来源
OpenCode ses_f717

[查看原始 Session]
[查看处理过程]
[导出到 SiYuan]
```

---

# 31. Raw Session 页面

新增：

```text
工作记录
```

页面：

```text
工作记录

全部
OpenCode
Codex
Gemini
Claude

项目
时间
状态
```

---

# 32. 数据源页面职责收窄

“数据源”只负责：

```text
Provider Detection
Connection Status
Session Count
Last Scan
Scan Now
```

不要再承担：

```text
知识整理
```

---

# 33. Scan 与 Pipeline 分离

必须明确：

```text
Scan
```

只做：

```text
发现源数据
```

然后：

```text
Pipeline
```

负责处理。

所以流程：

```text
Scan
↓
Discover Session
↓
enqueue pipeline
```

---

# 34. “同步”词汇建议弱化

V3 UI 不再强调：

```text
同步
```

因为这个词太模糊。

推荐：

```text
扫描
处理
知识
```

后台仍可以有 sync，但用户层少使用。

---

# 35. 主导航重构

推荐：

```text
概览

工作记录

知识库

处理中心

搜索

────────

数据源

设置

帮助与诊断
```

---

# 36. 概览页

建议：

```text
工作记录
660

知识
238

处理中
12

异常
3
```

下面：

```text
最近知识
最近工作记录
处理状态
AI / Embedding 状态
```

---

# 37. 不再暴露 SiYuan UI

正式模式：

```text
不默认提供完整 SiYuan Stage
```

可以在：

```text
高级
```

保留：

```text
[打开 SiYuan 管理界面]
```

只作为高级工具 / 数据维护。

---

# 38. SiYuan 在 V3 的角色

推荐三个阶段。

## 阶段 A

保留 SiYuan Runtime：

```text
作为旧数据兼容和导出
```

但主 UI 不依赖。

## 阶段 B

新增：

```text
Export to SiYuan
```

## 阶段 C

确认 AIKS 自身 Knowledge/Search 稳定后：

可以考虑：

```text
完全可选安装 SiYuan Runtime
```

减少 Installer 体积。

---

# 39. 开发体验必须重构

当前问题：

```text
改一个页面
↓
build release
↓
安装 EXE
↓
测试
```

不可接受。

正常研发必须分成三个模式。

---

# 40. Mode 1：Frontend Mock Dev

启动：

```powershell
cd apps\aiks-desktop
npm run dev
```

浏览器：

```text
http://localhost:5173
```

支持 Vite HMR：

```text
保存
↓
立即刷新
```

---

# 41. Mock API

前端不能直接到处：

```ts
invoke("xxx")
```

新增统一接口：

```text
src/api/
├── index.ts
├── types.ts
├── tauri.ts
└── mock.ts
```

定义：

```ts
export interface AiksApi {
  getOverview(): Promise<Overview>;
  getSessions(): Promise<SessionPage>;
  getSessionDetail(id: string): Promise<SessionDetail>;
  getPipelineRuns(): Promise<PipelineRun[]>;
  getPipelineDetail(id: string): Promise<PipelineDetail>;
  getKnowledge(): Promise<KnowledgePage>;
  getKnowledgeDetail(id: string): Promise<KnowledgeDetail>;
  search(query: string): Promise<SearchResult[]>;
}
```

---

# 42. MockAiksApi

开发时：

```text
VITE_AIKS_MOCK=true
```

使用：

```text
MockAiksApi
```

提供真实风格数据：

```text
660 Sessions
238 Knowledge
12 Processing
3 Failed
```

---

# 43. TauriAiksApi

正式模式：

```text
TauriAiksApi
```

内部才调用：

```ts
invoke(...)
```

页面不关心 Tauri。

---

# 44. Mode 2：Tauri Dev

启动：

```powershell
.\scripts\dev.ps1
```

或：

```powershell
npm run tauri dev
```

使用：

```text
Vite HMR
+
Tauri Rust Backend
```

改 React：

```text
无需重新打包
```

---

# 45. Mode 3：Release

只有准备正式发布才运行：

```powershell
.\scripts\build-release.ps1
```

---

# 46. 开发模式启动速度目标

建议：

```text
Frontend Mock:
< 3 秒

Tauri Dev:
< 10 秒（首次编译除外）

HMR:
< 1 秒
```

---

# 47. Dev Data

新增：

```text
fixtures/dev/
```

例如：

```text
short-session.json
long-session.json
tool-heavy-session.json
error-session.json
multi-knowledge-session.json
```

---

# 48. Pipeline Debug Mode

增加：

```text
AIKS_DEV_PIPELINE=true
```

可以指定：

```text
只处理 ses_f717
```

例如 CLI：

```powershell
aiks pipeline run ses_f717
```

---

# 49. CLI 新命令

推荐：

```text
aiks sessions list

aiks session show ses_f717

aiks pipeline run ses_f717

aiks pipeline status ses_f717

aiks chunk session ses_f717

aiks extract ses_f717

aiks embed ses_f717

aiks index ses_f717

aiks search "SiYuan Runtime"
```

---

# 50. Pipeline Stage 可单独重跑

例如 AI Extraction 失败：

不需要重新 Parse / Clean / Chunk。

支持：

```text
Retry from:
AI_EXTRACTED
```

---

# 51. 内容 Hash

每个阶段都要 Hash。

例如：

```text
session_hash
cleaned_hash
llm_chunk_hash
knowledge_hash
embedding_chunk_hash
embedding_model_version
```

避免重复执行。

---

# 52. 增量处理

规则：

```text
Source Session hash unchanged
↓
不重新 Parse
```

```text
AI Prompt changed
↓
重新 Extract
↓
重新 Knowledge Chunk
↓
重新 Embedding
```

```text
Embedding Model changed
↓
只重新 Embedding + Index
```

---

# 53. Pipeline Version

配置：

```text
pipeline_version=v3
cleaner_version=v1
session_chunker_version=v1
extractor_version=v2
prompt_version=knowledge-v2
embedding_chunker_version=v1
```

---

# 54. AI 与 Embedding 配置分离

不要只有一个“AI Settings”。

而是：

```text
AI 提炼模型

Embedding 模型
```

分别 Health Check。

---

# 55. 设置页

推荐包含：

```text
常规

处理

AI 提炼

Embedding

搜索

数据

高级
```

---

# 56. AI 提炼设置

```text
启用 AI 提炼

模型：
Qwen3.8-27B

Endpoint：
公司内部 AI

Temperature：
0.1

Chunk Size：
30k tokens

最大 Knowledge Items：
8

自动处理新 Session：
ON
```

---

# 57. Embedding 设置

```text
启用向量化

模型：
<embedding model>

Dimensions：
1024

Chunk：
800 tokens

Overlap：
120

Batch Size：
16
```

---

# 58. Processing Policy

可以控制：

```text
全部 Session

或者：

只处理：
>= 3 messages
>= 500 chars
```

---

# 59. Knowledge Score

仍可保留，但含义调整：

```text
Knowledge Extraction 前
```

模型判断：

```text
worth_extracting
```

如果 false：

```text
Session 状态：
READY_RAW
```

不生成 Knowledge。

---

# 60. Pipeline 最终状态

建议：

```text
RAW_ONLY

READY

PROCESSING

FAILED
```

其中：

```text
RAW_ONLY
```

代表：

```text
Session 已保存
但不值得提炼
```

---

# 61. Search Scope

搜索支持：

```text
Knowledge Only

Raw Sessions

Both
```

默认：

```text
Knowledge 优先
```

---

# 62. RAG 第一阶段不做复杂 Agent

搜索完成后可以提供：

```text
基于结果问 AI
```

但第一版只：

```text
Search
↓
Top K Chunks
↓
Qwen3.8-27B
↓
Answer
```

不要做多 Agent。

---

# 63. 观测性

新增 Diagnostics，展示：

```text
Provider
Pipeline
AI
Embedding
Vector DB
SQLite
```

---

# 64. Pipeline Metrics

统计：

```text
Sessions Processed
Knowledge Generated
Chunks Generated
Embeddings Generated
Failures
Average AI Latency
Average Embedding Latency
```

---

# 65. 日志

统一结构日志：

```text
[SCAN]
[PARSE]
[CLEAN]
[CHUNK]
[AI]
[KNOWLEDGE]
[EMBED]
[INDEX]
[SEARCH]
```

---

# 66. 日志示例

```text
[SCAN] OpenCode found 518 sessions
[PARSE] ses_f717 messages=140
[CLEAN] ses_f717 140 -> 126 messages
[CHUNK] ses_f717 llm_chunks=6
[AI] ses_f717 model=Qwen3.8-27B latency=18.4s items=4
[KNOWLEDGE] ses_f717 items=4
[EMBED] knowledge=kn_123 chunks=4 success=4
[INDEX] knowledge=kn_123 vectors=4
```

---

# 67. 数据迁移

V2.5 已有：

```text
source_session
sync_target
knowledge_extraction
```

V3 不应直接删除。

建议：

```text
migration 002_v3_pipeline.sql
```

新增表并迁移。

---

# 68. V2.5 Raw Session 兼容

现有已同步 Session：

可以重新构建 V3 Pipeline：

```text
source_session
↓
重新 Parse / Normalize
↓
Pipeline
```

---

# 69. SiYuan 已有 Knowledge 处理

先保留。

不要自动删除。

可以标记：

```text
legacy knowledge
```

后续提供：

```text
Import Legacy Knowledge
```

---

# 70. UI 风格

继续：

```text
企业内部
简洁
高信息密度
低饱和
```

但要增加：

```text
处理过程感
```

即：

```text
状态
步骤
进度
Trace
```

---

# 71. 首页推荐布局

```text
AIKS

工作记录 660
知识 238
处理中 12
失败 3

────────────────

正在处理

AIKS Pipeline 重构
AI 提炼中
Qwen3.8-27B
42%

────────────────

最近知识

PowerShell NativeCommandError
AIKS

SiYuan Runtime 集成
AIKS

────────────────

最近工作记录
```

---

# 72. 产品术语统一

建议 UI：

```text
Session
→ 工作记录

Pipeline
→ 处理流程

Knowledge Item
→ 知识

Embedding
→ 向量化

Vector Index
→ 向量索引
```

高级模式仍显示英文术语。

---

# 73. 开发者模式

设置：

```text
开发者模式
```

开启后显示：

```text
Session ID
Hash
Token Count
Prompt
Raw JSON
Embedding Dimension
Vector Score
SQLite IDs
```

---

# 74. 普通用户隐藏技术细节

普通模式只显示：

```text
解析
整理
向量化
完成
```

---

# 75. V3 P0

必须完成：

```text
统一 AIKS Knowledge UI

工作记录页面

处理中心

Session Detail

Pipeline Trace

LLM Chunk 可视化

AI Extraction Trace

KnowledgeItem 0~N

Embedding Chunk

Embedding Stage

Vector Index

统一搜索

Mock Frontend Dev

Tauri Dev HMR

Release 与 Dev 分离
```

---

# 76. V3 P1

```text
Hybrid Search

项目视图

收藏

搜索问答

Pipeline Metrics

历史重处理

开发者模式

Legacy SiYuan Export
```

---

# 77. V3 P2

```text
团队知识

中央同步

SSO

权限

团队搜索

知识推荐

团队 Agent
```

---

# 78. 明确暂缓

V3 暂不做：

```text
Knowledge Graph

复杂 Agent

Workflow Engine

云同步

团队权限

多人编辑

复杂 Reranker Pipeline
```

---

# 79. 不推翻现有模块

继续复用：

```text
Provider
NormalizedSession
Watcher
Sanitizer
Archive
Qwen Client
Tauri Shell
Installer
SQLite
```

需要重构：

```text
Sync 后面的 Knowledge 层
UI 信息架构
Search / Embedding 控制权
Dev Workflow
```

---

# 80. 推荐目录

```text
crates/aiks-core/src/
├── providers/
├── model/
├── ingestion/
├── pipeline/
│   ├── cleaner.rs
│   ├── session_chunker.rs
│   ├── orchestrator.rs
│   ├── ai_stage.rs
│   ├── embedding_chunker.rs
│   ├── embedding_stage.rs
│   └── index_stage.rs
├── knowledge/
├── search/
│   ├── fts.rs
│   ├── vector.rs
│   └── hybrid.rs
├── embedding/
├── ai/
├── storage/
└── watcher/
```

---

# 81. React 推荐目录

```text
src/
├── api/
│   ├── index.ts
│   ├── tauri.ts
│   ├── mock.ts
│   └── types.ts
├── pages/
│   ├── OverviewPage.tsx
│   ├── SessionsPage.tsx
│   ├── SessionDetailPage.tsx
│   ├── KnowledgePage.tsx
│   ├── KnowledgeDetailPage.tsx
│   ├── ProcessingPage.tsx
│   ├── ProcessingDetailPage.tsx
│   ├── SearchPage.tsx
│   ├── SourcesPage.tsx
│   ├── SettingsPage.tsx
│   └── DiagnosticsPage.tsx
├── components/
└── mocks/
```

---

# 82. 新 Tauri Commands

推荐：

```text
list_sessions
get_session_detail

list_pipeline_runs
get_pipeline_detail
retry_pipeline

list_knowledge
get_knowledge_detail

search_knowledge

get_ai_health
get_embedding_health

run_pipeline_for_session

get_pipeline_metrics
```

---

# 83. CLI Dev Commands

```text
aiks scan

aiks sessions list

aiks session show <id>

aiks pipeline run <id>

aiks pipeline retry <id> --from ai

aiks chunks session <id>

aiks chunks knowledge <id>

aiks extract <id>

aiks embed <id>

aiks search "<query>"
```

---

# 84. 真实 E2E 验收

选择：

```text
ses_f717
```

必须完整：

```text
OpenCode SQLite
↓
Parse
↓
140 Messages
↓
Clean
↓
LLM Chunk
↓
6 Chunks
↓
Qwen3.8-27B
↓
N Knowledge Items
↓
Knowledge Chunk
↓
Embedding
↓
Vector Index
↓
Search
```

每一步 UI 可见。

---

# 85. Search 验收

输入：

```text
PowerShell NativeCommandError
```

应：

```text
FTS
+
Vector
↓
找到相关 Knowledge
```

点击结果：

```text
Knowledge
↓
Source Trace
↓
ses_f717
```

---

# 86. Debug 验收

前端 UI：

```powershell
npm run dev
```

不需要：

```text
EXE
Installer
Tauri
```

即可调页面。

---

# 87. Tauri Dev 验收

```powershell
npm run tauri dev
```

修改 React：

```text
HMR
```

无需重新打包。

---

# 88. Release 验收

只有：

```powershell
.\scripts\build-release.ps1
```

才打 EXE。

---

# 89. 升级安装问题继续保留

上一轮发现：

```text
SiYuan-Kernel.exe 正在运行
↓
Installer 无法覆盖
```

这个问题继续作为 P0。

升级时：

```text
检测 AIKS
↓
优雅退出
↓
停止 Pipeline Worker
↓
停止 AI / Embedding Queue
↓
停止 SiYuan（如果仍保留）
↓
等待
↓
必要时安全强杀
↓
安装
```

---

# 90. 实施顺序建议

## Phase 1：开发体验

先完成：

```text
Mock API
Frontend Dev
Tauri Dev
```

目标：

```text
以后调页面不再打包 EXE
```

## Phase 2：Pipeline Data Model

实现：

```text
pipeline_run
pipeline_stage_run
session_chunk
knowledge_item
knowledge_chunk
embedding_record
```

## Phase 3：Processing Center

先用 Mock / Existing Data 展示完整 Pipeline。

## Phase 4：AI Extraction

升级成：

```text
1 Session → 0~N KnowledgeItems
```

## Phase 5：Embedding

接入真正 Embedding Model。

## Phase 6：Vector Search

```text
sqlite-vec
+
FTS5
```

## Phase 7：Knowledge UI

AIKS 自己展示 Knowledge。

## Phase 8：弱化 SiYuan

变成：

```text
高级
→ 导出到 SiYuan
```

---

# 91. 版本策略

建议：

```text
V2.5
维持现状，只修严重 Bug

V3.0-alpha
Unified Pipeline

V3.0-beta
内部试用

V3.0
正式内部推广
```

---

# 92. V3 Alpha 验收标准

必须满足：

```text
[ ] 不打 EXE 就能调前端

[ ] Tauri Dev 支持 HMR

[ ] 能查看所有 Session

[ ] 能查看单 Session 原始消息

[ ] 能看到 Cleaner 输出

[ ] 能看到 LLM Chunk

[ ] 能看到 AI Extraction

[ ] 1 Session 可输出多个 Knowledge

[ ] 能看到 Embedding Chunk

[ ] 能看到 Embedding 状态

[ ] 能看到 Vector Index 状态

[ ] 能搜索

[ ] 搜索能回溯 Session

[ ] SiYuan 不再是普通用户唯一知识入口
```

---

# 93. 一句话总结

当前 AIKS 最大的问题不是“页面不好看”，而是：

> 数据采集、知识处理和知识消费被拆成了几个彼此割裂的模块。

V3 的核心不是继续增加功能，而是把整个链路重新统一成：

```text
Session
↓
Processing
↓
Knowledge
↓
Search
```

并且让整个过程：

```text
看得见
可调试
可重跑
可追踪
```

这才是 AIKS 下一阶段最值得投入的方向。
