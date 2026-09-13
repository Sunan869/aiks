# AIKS V3 改造实际情况与后续计划

版本：2026-09-13  
改造基线：V2.5（commit 06075ed）  
改造成果：V3 alpha（commit 642d377）  
验收结果：**构建成功** — `AIKS_0.2.0_x64-setup.exe (69.8 MB)`

---

## 一、本次改造实际完成情况

### 1.1 已完成模块

#### Git 基础
- ✅ 更新 `.gitignore`（Rust/Node/Tauri/IDE 全覆盖）
- ✅ 提交全项目代码基线（commit 06075ed，159 个文件）
- ✅ 提交 V3 改造成果（commit 642d377，29 个文件）

#### 数据层（Rust）
- ✅ `002_v3_pipeline.sql`：新增 8 张核心表
  - `pipeline_run` — 流水线运行记录
  - `pipeline_stage_run` — 阶段运行详情
  - `session_chunk` — LLM 分块记录
  - `knowledge_item` — V3 知识条目（0~N per session）
  - `knowledge_chunk` — 向量切片
  - `embedding_record` — Embedding 向量
  - `ai_request_log` — AI 调用日志
  - `knowledge_fts` / `session_fts` — FTS5 全文索引
- ✅ 数据库自动执行 V1 + V3 两阶段迁移（`StateDb::open`）
- ✅ `model/pipeline.rs`：V3 核心数据类型
  - `PipelineStage` 枚举（13 个状态，带 next() 流转）
  - `KnowledgeItem`（1 Session → 0~N 知识条目）
  - `KnowledgeChunk`、`EmbeddingRecord`、`EmbeddingConfig`
  - `PipelineStats`
- ✅ `pipeline/` 模块
  - `orchestrator.rs` — 流水线入队/状态追踪
  - `cleaner.rs` — Session 清洗（移除噪声保留关键信息）
  - `repo.rs` — SQLite 读写封装
  - `status.rs` — 前端展示 DTO

#### 测试
- ✅ 74 个 Rust 单元测试全部通过
  - 包含新增 V3 测试：`pipeline_stage_roundtrip`、`pipeline_stage_sequence`、`knowledge_category_from_str`、`clean_removes_empty`、`clean_preserves_error_tool_result`

#### Tauri 后端命令
- ✅ 新增 6 个 V3 命令
  - `list_pipeline_runs` — 处理中心列表
  - `get_pipeline_detail` — 流水线详情（含 stage trace）
  - `get_pipeline_stats` — 统计摘要
  - `list_sessions_v3` — 工作记录列表（含流水线状态）
  - `list_knowledge` — 知识条目列表
  - `search_knowledge` — 全文搜索（FTS5 + LIKE 降级）

#### 前端 API 抽象层
- ✅ `src/api/types.ts` — 统一类型定义
- ✅ `src/api/index.ts` — `AiksApi` 接口 + Mock 检测
- ✅ `src/api/tauri.ts` — `TauriAiksApi`（调用 invoke）
- ✅ `src/api/mock.ts` — `MockAiksApi`（660 会话 / 238 知识条目假数据）
- ✅ `src/api/client.ts` — 单例，根据环境自动选择实现

**Mock 模式启用方式：**
```powershell
cd apps\aiks-desktop
$env:VITE_AIKS_MOCK="true"; npm run dev
# 访问 http://localhost:1420
# 无需 EXE 即可调试所有页面
```

#### 前端页面（V3 新增）
- ✅ `SessionsPage.tsx` — 工作记录列表（含来源筛选、流水线状态标签、分页）
- ✅ `KnowledgeBasePageV3.tsx` — 知识库（卡片视图、分类筛选、标签、置信度）
- ✅ `ProcessingPage.tsx` — 处理中心（统计面板 + 流水线状态矩阵）
- ✅ `SearchPage.tsx` — 全文搜索页（回车触发、结果卡片、置信度展示）

#### 前端导航重构
- ✅ `Sidebar.tsx` — V3 导航结构
  ```
  概览
  工作记录   ← 新增
  知识库     ← 重构（原 SiYuan 页面）
  处理中心   ← 新增
  搜索       ← 新增
  ──────
  数据源
  设置
  帮助与诊断
  ```
- ✅ `App.tsx` — 统一使用 `getApi()` 抽象，Mock 模式标识

### 1.2 已验收指标

| 验收项 | 状态 |
|-------|------|
| `scripts\build-release.ps1` 打包出 EXE | ✅ AIKS_0.2.0_x64-setup.exe 69.8 MB |
| 74 个 Rust 单元测试通过 | ✅ |
| TypeScript 无编译错误 | ✅ |
| Mock 模式前端可独立运行 | ✅ |
| V3 DB 迁移不破坏 V2.5 数据 | ✅（IF NOT EXISTS 安全） |

---

## 二、当前架构状态

```
AIKS V3 Alpha
│
├── 数据层 ──────────────────────────────────────── ✅
│   ├── V1 表：source_session, sync_target, ...（保留）
│   └── V3 表：pipeline_run, knowledge_item, ...（新增）
│
├── Pipeline 模块 ───────────────────────────────── ✅（骨架）
│   ├── orchestrator.rs（入队/状态追踪）
│   ├── cleaner.rs（消息清洗）
│   └── repo.rs（DB 读写）
│
├── Tauri 命令层 ────────────────────────────────── ✅
│   ├── V2.5 命令（全部保留）
│   └── V3 命令（list_sessions_v3, list_knowledge, search_knowledge 等）
│
├── 前端 API 层 ─────────────────────────────────── ✅
│   ├── Mock API（开发用，不依赖 Tauri）
│   └── Tauri API（生产用）
│
└── 前端页面 ────────────────────────────────────── ✅
    ├── 概览 / 数据源 / 设置 / 诊断（V2.5 保留）
    ├── 工作记录（新增）
    ├── 知识库 V3（新增）
    ├── 处理中心（新增）
    └── 搜索（新增）
```

### 尚未接通的部分（架构预留，待后续实现）

| 模块 | 现状 | 说明 |
|------|------|------|
| Pipeline 实际执行 | 骨架已建，未自动触发 | 现有 AI 提炼走 V2.5 路径 |
| LLM Chunker | 未实现（ai/chunker.rs 存在旧版） | 需重写为 V3 `session_chunker.rs` |
| AI Extraction V3 | 仍用 1 Session → 1 Knowledge 旧逻辑 | 需升级为 0~N KnowledgeItems |
| Embedding Stage | 框架预留，`EmbeddingConfig.enabled=false` | 待接入 Embedding 模型 |
| Vector Store (sqlite-vec) | 未引入 | 待 Embedding 接入后实现 |
| Hybrid Search | 仅 LIKE 降级，FTS5 等待数据 | 有数据后自动启用 |
| SiYuan 弱化 | SiYuan 仍为主 Sink | V3 Phase 8 按计划逐步弱化 |
| CLI V3 命令 | 未实现新子命令 | `aiks pipeline run <id>` 等 |

---

## 三、后续计划

### Phase A：Pipeline 自动触发（2-3 天）

**目标**：扫描到新 Session 后自动进入 V3 Pipeline

任务：
1. 在 `sync_and_extract` 结束后调用 `PipelineOrchestrator::enqueue`
2. 实现 `session_chunker.rs`（30~50 消息/chunk 或 20k tokens/chunk）
3. 连接 `cleaner.rs` 到实际 Pipeline 流程

**验收**：扫描后，`list_pipeline_runs` 能看到 CLEANED 状态记录

---

### Phase B：AI Extraction V3（2-4 天）

**目标**：1 Session → 0~N KnowledgeItems

任务：
1. 修改 AI Prompt：输出包含 `items[]` 数组的 JSON
2. 实现 `ai_stage.rs`：
   - 按 chunk 调用模型
   - 解析 0~N knowledge items
   - 写入 `knowledge_item` 表
3. 更新 `ai_request_log` 记录调用详情
4. 连接到 Pipeline orchestrator

**验收**：一个有 140 条消息的 Session → 4 个 KnowledgeItems

---

### Phase C：Embedding 接入（3-5 天）

**前提**：公司内部 vLLM 部署 Embedding 模型

任务：
1. 实现 `embedding_chunker.rs`（800 tokens/chunk，120 overlap）
2. 实现 `embedding_stage.rs`（调用 `/v1/embeddings` API）
3. 引入 `sqlite-vec` crate
4. 实现 `index_stage.rs`（向量写入 SQLite）
5. 实现 `search/vector.rs` + `search/hybrid.rs`

**验收**：搜索 "PowerShell NativeCommandError" → 找到相关知识

---

### Phase D：处理中心 UI 完善（1-2 天）

任务：
1. Session Detail 页面（点击工作记录查看原始消息）
2. Pipeline Detail 页面（点击处理任务查看各阶段耗时）
3. 重试功能（从指定阶段重新处理）

---

### Phase E：知识库 UI 完善（1-2 天）

任务：
1. KnowledgeDetailPage（标题、摘要、根因、解决方案、关键命令）
2. 来源溯源（知识 → 原始 Session）
3. 标签筛选、项目分组

---

### Phase F：SiYuan 弱化（1 天）

任务：
1. 将 SiYuan UI 入口移至「设置 → 高级」
2. 添加「导出到 SiYuan」按钮（在 KnowledgeDetail）
3. 默认不启动 SiYuan Runtime（保留兼容选项）

---

### 版本里程碑

```
V3.0-alpha （当前）
  ✅ 数据模型 + UI 骨架
  ✅ Mock 前端开发模式
  ✅ build-release.ps1 验收

V3.0-beta （Phase A + B 完成后）
  目标：Session → KnowledgeItems 全链路跑通
  
V3.0 （Phase C + D + E 完成后）
  目标：Embedding + 向量搜索 + 完整 UI
  
V3.1 （Phase F 完成后）
  目标：SiYuan 正式成为可选 Export
```

---

## 四、开发模式说明

### Mode 1：纯前端 Mock 开发（秒启动）
```powershell
cd apps\aiks-desktop
$env:VITE_AIKS_MOCK="true"; npm run dev
# http://localhost:1420
# 支持 HMR，无需 EXE 或 Tauri
```

### Mode 2：Tauri 开发模式（HMR）
```powershell
.\scripts\dev.ps1
# 或
cd apps\aiks-desktop && npm run tauri dev
# 修改 React 代码 → 自动热更新
# 无需 build-release.ps1
```

### Mode 3：Release 打包（仅正式发布时）
```powershell
.\scripts\build-release.ps1
# 输出：target\release\bundle\nsis\AIKS_0.2.0_x64-setup.exe
```

---

## 五、关键文件索引

| 文件 | 说明 |
|------|------|
| `crates/aiks-core/migrations/002_v3_pipeline.sql` | V3 数据库迁移 |
| `crates/aiks-core/src/model/pipeline.rs` | V3 核心数据类型 |
| `crates/aiks-core/src/pipeline/` | Pipeline 模块 |
| `apps/aiks-desktop/src/api/` | 前端 API 抽象层 |
| `apps/aiks-desktop/src/api/mock.ts` | Mock 数据（开发用）|
| `apps/aiks-desktop/src/pages/SessionsPage.tsx` | 工作记录页 |
| `apps/aiks-desktop/src/pages/ProcessingPage.tsx` | 处理中心页 |
| `apps/aiks-desktop/src/pages/KnowledgeBasePageV3.tsx` | 知识库页 |
| `apps/aiks-desktop/src/pages/SearchPage.tsx` | 搜索页 |
| `docs/AIKS_V3_Unified_Knowledge_Pipeline_详细改造方案.md` | 完整 V3 设计文档 |
