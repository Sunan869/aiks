# AIKS V4.1 SiYuan Embedded Workbench 设计

日期：2026-09-16  
状态：Design Approved in Chat / Pending Written Spec Review  
分支：`feature/v4.1-siyuan-embedded-workbench`

## 1. 背景

V4 Native Knowledge Workbench 将 AIKS Desktop 提升为知识产品入口，并补充了原生 Knowledge CRUD、收藏、归档、搜索与简单 Markdown 编辑。但实际使用后暴露出明显缺口：

- 当前 Knowledge Workbench 过于简化，不能替代 SiYuan 的 Block 编辑和知识组织能力。
- 原始 Session 与提炼知识之间的双向追溯体验被弱化。
- V4 同时在 SQLite 与 SiYuan 保存知识正文，形成双 Master 风险。
- 如果继续在 AIKS 内重写 Block Editor、双链、反链、大纲、Database、Graph，会重新造一套 SiYuan/Notion，成本和稳定性不可接受。

V4.1 改为：**AIKS Shell + SiYuan Knowledge Engine**。

AIKS 继续作为唯一产品入口，SiYuan 作为嵌入式知识内容源、编辑内核与知识工作区。

## 2. 核心原则

### 2.1 一个产品入口

用户始终运行 AIKS Desktop，不要求单独打开或理解 SiYuan。

AIKS 保留：

- 概览
- 数据源
- Scan / Sync / Extraction Pipeline
- AI 状态
- 全局智能搜索
- 设置
- 诊断
- 知识库入口

SiYuan 不再作为第二个独立应用暴露给普通用户。

### 2.2 一个知识内容源

用户正文、Block、标题、文档树、标签、附件、引用、Database 内容统一以 SiYuan 为 Source of Truth。

AIKS SQLite 保留控制面与可重建索引，不再作为知识正文 Master。

### 2.3 一个控制数据库

AIKS SQLite 继续负责：

- Provider Scan 状态
- source_session 映射
- parser version / content hash
- Sync / Extraction job
- retry / lease / diagnostics
- AI score
- SiYuan doc/block 映射
- migration 状态
- 本地列表缓存
- FTS / embedding / rerank 索引

### 2.4 不 Fork SiYuan

不复制、不修改 SiYuan 源码，不维护 AIKS 专用 SiYuan Fork。

复用能力通过：

- SiYuan HTTP API
- SiYuan Plugin API
- AIKS Bridge Plugin
- CSS/Layout Adapter
- Embedded WebView

完成。

## 3. 总体架构

```text
                    AIKS Desktop
                         │
         ┌───────────────┴────────────────┐
         │                                │
     React Shell                    SiYuan WebView
         │                                │
 Overview / Sources / Pipeline      Tree / Protyle / Tabs
 AI / Search / Settings             Outline / Backlinks
 Diagnostics                        Database / Graph
         │                                │
         └──── Tauri Workbench Bridge ────┘
                         │
                   SiYuan Kernel
                         │
                 AI Knowledge
              ┌──────────┴──────────┐
              │                     │
      10 AI Sessions          20 Knowledge
              │                     │
          Raw Session        AI + Manual Knowledge
```

核心职责：

```text
AIKS 决定“我要做什么”
SiYuan 决定“知识怎么浏览、组织和编辑”
```

## 4. SiYuan Notebook 结构

V4.1 统一使用一个 Notebook：

```text
AI Knowledge
├── 10 AI Sessions
│   ├── OpenCode
│   ├── Codex
│   ├── Claude
│   └── Gemini
│
├── 20 Knowledge
│   ├── Projects
│   ├── Manual
│   └── Inbox
│
└── 90 System
```

废弃“AI Knowledge + AI Session Archive 两个 Notebook 作为长期目标”的设计。

迁移时优先移动已有 Session 文档而非重新创建，以保留 doc/block ID 与已有引用。

## 5. 内容与控制数据模型

### 5.1 source_session

继续由 AIKS SQLite 管理业务映射与采集状态，至少保留：

```text
id
source
external_session_id
source_path
project_name
parser_version
content_hash
siyuan_doc_id
sync_status
last_seen_at
updated_at
```

正文不以 SQLite 为 Master。

### 5.2 knowledge_item

V4.1 中 knowledge_item 逐步收敛为控制实体：

```text
id
siyuan_doc_id
source_session_id
source_type       conversation | manual
managed_by        pipeline | user
status            active | archived
knowledge_score
generated_hash
current_remote_hash
migration_status
created_at
updated_at
```

V4 已有：

```text
title
summary
content
tags
category
project_name
```

在 V4.1 第一版暂时保留为 legacy snapshot/cache，不再作为正文写入主路径。

真正删除这些 legacy 正文字段放到后续版本执行。

### 5.3 SiYuan 自定义属性

Knowledge 文档至少写入：

```text
custom-aiks-id
custom-aiks-source-type
custom-aiks-managed-by
custom-aiks-session-id
custom-aiks-project
custom-aiks-category
custom-aiks-generated-hash
```

Raw Session 至少写入：

```text
custom-aiks-managed=true
custom-aiks-kind=session
custom-aiks-source
custom-aiks-session-id
custom-aiks-content-hash
custom-aiks-parser-version
custom-aiks-synced-at
```

## 6. Knowledge 与 Session 的双向追溯

SQLite 层仍保留：

```text
knowledge_item.source_session_id
```

同时 Knowledge 文档中必须写入真实 SiYuan Block Reference：

```text
来源会话：((<session-doc-or-block-id> "查看原始 Session"))
```

形成两层关系：

1. AIKS 可靠业务关系：用于 Pipeline、迁移与一致性校验。
2. SiYuan 原生引用关系：用于点击跳转、反链、Graph 与知识浏览。

Knowledge → Session：通过 Bridge 打开原始 Session，可进一步定位 message block。

Session → Knowledge：优先使用 SiYuan 反链，同时 AIKS 可显示“本会话已提炼 N 条知识”和“重新提炼”。

## 7. Workspace 页面边界

### 7.1 AIKS 原生页面

以下继续由 AIKS React 实现：

- 概览
- 数据源
- 处理中心
- AI 状态
- 全局智能搜索
- 设置
- 诊断
- 知识库一级入口

### 7.2 SiYuan Embedded Workbench

知识库内部直接复用 SiYuan：

- 文档树
- Tabs
- Protyle Block Editor
- Slash Menu
- Code Block
- 表格
- 数学公式
- 图片/附件
- Block 拖拽
- Fold / Unfold
- Block Zoom
- 面包屑
- Outline
- Tags
- Block Reference
- Backlinks
- Embed Block
- SiYuan 原生全文搜索
- Database
- Relation
- Rollup
- Table / Gallery
- Graph
- 历史/快照
- 导出

不在 AIKS 中重复实现上述通用知识编辑能力。

## 8. Workspace Mode

### 8.1 Knowledge Mode

目标目录：`/20 Knowledge`

允许完整编辑：

- 新建
- 编辑
- 删除
- Block 操作
- 标签
- 双链
- Database
- 附件
- Graph

Manual 与 AI 提炼知识在 SiYuan 中都是正常文档，仅通过属性区分来源与管理策略。

### 8.2 Session Mode

目标目录：`/10 AI Sessions`

默认只读。

允许：

- 搜索
- Copy
- Fold
- Block Zoom
- Outline
- Backlinks
- Graph
- 查看引用
- 跳转知识

禁止直接把 Raw Session 当普通笔记任意编辑。

Session 顶部提供 AIKS 动作：

- 创建相关知识
- 智能提炼
- 重新提炼

如果检测到 Raw Session 被意外修改，不立即覆盖，提供：

- 恢复原始内容
- 将修改保存为 Knowledge 后恢复 Session

## 9. Embedded WebView

AIKS 主窗口使用两个逻辑 WebView：

```text
WebView A = AIKS React Shell
WebView B = SiYuan Workbench
```

SiYuan WebView 在 Kernel Ready 后创建一次，整个应用生命周期保持存在。

进入知识库、Session/Knowledge 详情、搜索结果详情时显示；进入概览、数据源、处理中心、设置等页面时隐藏。

禁止每次路由切换销毁并重载 SiYuan，否则会丢失：

- Tab
- 光标
- 当前文档
- 文档树展开状态
- 折叠状态
- 编辑上下文

旧版独立 Knowledge Window 保留为降级路径，不作为默认体验。

## 10. AIKS Bridge 架构

主通信链：

```text
AIKS React
    ↓
Tauri Command/Event
    ↓
WorkbenchController (Rust)
    ↓
SiYuan Child WebView
    ↓
AIKS Bridge Plugin
    ↓
SiYuan UI
```

业务控制逻辑必须放在 Rust/AIKS Core，不依赖 SiYuan 页面生命周期。

Bridge Plugin 负责：

- UI 打开/定位
- Workspace Mode
- 前端事件
- Layout/CSS Adapter
- SiYuan Plugin API 适配

## 11. Bridge Protocol V1

所有消息带协议版本与 requestId：

```json
{
  "version": 1,
  "requestId": "...",
  "action": "openBlock",
  "payload": {}
}
```

### AIKS → SiYuan

至少支持：

```text
showKnowledgeRoot
showSessionRoot
openDocument
openBlock
focusBlock
createKnowledge
createKnowledgeFromSession
showBacklinks
showOutline
showDatabase
showGraph
showSearch
setWorkspaceMode
setTheme
refreshDocument
```

### SiYuan → AIKS

至少支持：

```text
bridgeReady
documentOpened
documentChanged
documentCreated
documentDeleted
blockFocused
knowledgeModified
requestReExtract
requestOpenSession
requestOpenKnowledge
requestShowPipeline
```

Bridge 不传输每次键盘输入，也不以事件正文作为知识 Master。

`documentChanged` 触发 AIKS cache/index invalidation，debounce 后重新读取 SiYuan 内容并更新索引。

## 12. Bridge 安全

SiYuan WebView 不获得无限制 Tauri IPC 权限。

Bridge 事件必须校验：

- WebView label
- loopback origin
- runtime nonce / handshake
- protocol version
- payload schema

启动流程：

```text
AIKS 生成 nonce
→ 创建 Embedded SiYuan WebView
→ Bridge Plugin handshake
→ BRIDGE_READY
→ 才允许业务消息
```

仅允许 AIKS 管理的 loopback Embedded SiYuan 地址。

## 13. SiYuan UI 裁剪

原则：不 Fork SiYuan。

通过 Bridge Plugin + CSS + Adapter 隐藏不属于 AIKS 的全局入口。

### 保留

- 文档树
- Tabs
- Breadcrumb
- Protyle
- Block Toolbar
- Slash Menu
- Outline
- Backlinks
- Tags
- Search
- Database
- Graph
- History
- Assets
- Export

### 默认隐藏

- SiYuan Logo/顶级应用导航
- 账号
- 云同步入口
- 集市
- 插件管理
- SiYuan 全局设置
- 关于 SiYuan
- 与 AIKS 重复的系统入口

新增 `SiyuanAdapter` 隔离 SiYuan UI/API 差异：

```text
openDocument
focusBlock
setReadOnly
showBacklinks
showOutline
showGraph
applyAiksLayout
```

升级 SiYuan 时优先只修改 Adapter。

Adapter 初始化失败时降级到完整 SiYuan Workbench，不允许整个知识库白屏。

## 14. 创建与编辑语义

### 14.1 手动知识

V4：

```text
AIKS textarea → SQLite → Publish → SiYuan
```

V4.1：

```text
+ 新建知识
→ SiYuan createDoc
→ 打开 Protyle
→ AIKS 记录 doc_id 与控制元数据
```

用户从创建开始即直接编辑 SiYuan Master。

### 14.2 AI 提炼知识

```text
Raw Session
→ Extraction Pipeline
→ Knowledge JSON
→ Renderer
→ create/update SiYuan Document
→ 记录 siyuan_doc_id / generated_hash
```

### 14.3 用户编辑保护

AI 写入后记录：

```text
generated_hash = A
```

之后重新提炼前读取 SiYuan 当前 hash。

如果：

```text
current_hash == generated_hash
```

允许自动更新。

如果：

```text
current_hash != generated_hash
```

说明用户或其他操作修改过知识正文：

```text
managed_by=user
→ 不自动覆盖
→ NEED_REVIEW / CONFLICT
```

提供：

- 查看差异
- 保留我的版本
- 采用 AI 新版本
- 手工合并

## 15. V4 → V4.1 数据迁移

采用三阶段迁移：

### Phase A：只加不删

新增：

- `siyuan_doc_id`
- `generated_hash`
- `current_remote_hash`
- `migration_status`
- 必要映射/缓存表

V4 原正文列继续存在，但降级为 legacy snapshot/cache。

### Phase B：逐条迁移

#### 已存在 Raw Session SiYuan 文档

优先 move 到统一 Notebook 的 `/10 AI Sessions`，不重建文档。

#### 已 Publish 的 Knowledge

已有 `knowledge_sync_target.target_id` 的文档直接成为 Canonical。

补齐 V4.1 attributes 与映射。

#### 仅 SQLite 存在的 Knowledge

创建 `/20 Knowledge/...` SiYuan 文档，得到 doc_id 后绑定。

#### 双边都修改过

若 SQLite 与 SiYuan 均偏离上次 baseline：

```text
MIGRATION_CONFLICT
```

禁止自动覆盖。

### Phase C：切换 Master

仅当全部可迁移内容成功、冲突被显式处理后：

```text
content_master = siyuan
migration_completed = true
```

从此正常业务不再从 SQLite legacy content 写回 SiYuan。

## 16. 升级备份与回滚

V4.1 首次迁移前创建 pre-migration backup/checkpoint，至少覆盖：

- `aiks.db`
- SiYuan workspace/checkpoint
- migration manifest

manifest 包含：

```text
total_sessions
migrated_sessions
total_knowledge
migrated_knowledge
conflicts
failed
started_at
completed_at
```

迁移必须：

- 可重复执行
- 单条失败不终止全部
- 不删除未确认数据
- 未完成前不移除旧字段
- 未完成前不删除旧 Notebook

## 17. Read Model 与搜索索引

SiYuan 是正文 Master，但 AIKS 为性能保留可重建缓存：

```text
knowledge_index
├─ knowledge_id
├─ siyuan_doc_id
├─ title_cache
├─ project_cache
├─ category_cache
├─ tags_cache
├─ updated_at
└─ ...
```

列表、筛选、统计允许读 cache。

正文详情始终由 SiYuan Workbench 展示。

AIKS Embedding Pipeline 改为：

```text
SiYuan Document/Blocks
→ extract plain/markdown content
→ chunk
→ embedding
→ AIKS vector index
```

SiYuan 内容变化后使对应 FTS/embedding index 失效并异步重建。

## 18. 搜索职责

### AIKS Search

负责跨域智能检索：

- Knowledge
- Session
- FTS
- Embedding
- Rerank
- Project
- Source
- Time
- Category

搜索结果必须尽量携带：

```text
siyuan_doc_id
siyuan_block_id
```

点击结果后通过 Bridge 打开并定位。

### SiYuan Search

保留原生知识库搜索：

- Block
- Attr
- Tag
- Reference
- 当前 Knowledge Workspace 全文

两者互补，不互相替代。

## 19. Raw Session 只读保护

只读必须两层实现：

1. Bridge Plugin / SiyuanAdapter 在 Session Mode 限制编辑动作。
2. Sync 层比较 Raw Session generated/content hash 与 SiYuan current hash。

若检测人工修改：

```text
RAW_SESSION_MODIFIED
```

不得无提示覆盖。

建议动作：

```text
恢复原始内容
将修改内容保存为 Knowledge 后恢复
```

## 20. 错误与降级

### Kernel 不可用

- AIKS 业务壳仍能启动。
- Overview/Sources/Diagnostics 可用。
- Knowledge Workbench 显示 Kernel unavailable，并提供重新连接。

### Bridge 未 Ready

- 不发送 UI 控制命令。
- Workbench 可降级展示完整 SiYuan 页面。

### Adapter 与 SiYuan 版本不兼容

- 禁止白屏。
- 关闭裁剪与高级定位功能。
- 保留完整 SiYuan Workbench 作为功能降级。

### AI 不可用

- Raw Session Sync 不受影响。
- 手动知识编辑不受影响。
- AI 提炼进入待处理/失败状态，可恢复后重试。

## 21. 测试策略

### Core 单元测试

必须覆盖：

- SiYuan Master 的 Knowledge 映射
- generated_hash/user-modified 检测
- migration idempotency
- 已发布/未发布/冲突三类迁移
- Raw Session modified detection
- cache invalidation
- embedding source from SiYuan

### Bridge Contract Test

覆盖：

- protocol version
- invalid action rejection
- invalid payload rejection
- nonce/handshake
- openDocument/openBlock/focusBlock
- workspace mode
- event delivery

### Desktop 测试

覆盖：

- Workbench WebView 生命周期
- show/hide 不销毁
- AIKS route ↔ SiYuan workspace mode
- dark/light theme sync
- Kernel unavailable fallback
- Adapter failure fallback

### Windows CI

继续保留 Windows Desktop compile job。

对新增 Tauri child WebView 相关 Rust 代码必须纳入 Windows 编译覆盖。

## 22. E2E 强制验收

使用真实本地数据至少验证：

1. AIKS 启动只出现一个产品主窗口。
2. Embedded SiYuan Kernel 正常启动。
3. `AI Knowledge/10 AI Sessions` 存在真实 OpenCode/Codex/Gemini Session。
4. Session 默认只读，可搜索、折叠、查看大纲/反链。
5. AI 提炼结果写入 `20 Knowledge`。
6. Knowledge 使用完整 Block Editor 编辑。
7. Knowledge 点击来源能回到原始 Session。
8. Session 反链能看到由它提炼/创建的 Knowledge。
9. AIKS 搜索命中 Knowledge 后能打开并定位 SiYuan Block。
10. AIKS 搜索命中 Session 后能打开并定位具体消息/Block。
11. Knowledge 用户编辑后重新提炼不会被 AI 自动覆盖。
12. Raw Session 被修改后不会静默覆盖。
13. Database 可创建并正常使用 Relation/Rollup/Table/Gallery。
14. Graph 可展示 Session/Knowledge 引用关系。
15. 在知识页面切换到设置再回来，当前文档/Tab/编辑状态不因 WebView 重建丢失。
16. SiYuan Kernel 暂停后 AIKS 其他业务页面仍可运行，恢复后 Workbench 可重连。
17. V4 已有手动 Knowledge 无损迁移到 SiYuan。
18. V4 已 Publish Knowledge 不重复创建。
19. 原 AI Session Archive 文档迁移后保留原 doc ID/引用关系。
20. 迁移过程中出现冲突时能人工处理，未处理数据不丢失。

## 23. 明确不做

V4.1 不做：

- AIKS 自研 Block Editor
- AIKS 自研 Database/Relation/Rollup
- AIKS 自研 Graph
- AIKS 自研完整反链系统
- SiYuan 源码 Fork
- 把 Pipeline/Provider 状态塞进 SiYuan 文档
- 删除 SQLite 控制数据库
- V4.1 第一版物理删除 legacy knowledge 正文字段

## 24. 实施顺序建议

后续实施计划应按以下依赖顺序展开：

```text
1. V4.1 schema + migration state
2. SiYuan canonical content adapter
3. Unified notebook/session migration
4. Knowledge migration + conflict handling
5. Persistent child WebView
6. Bridge Protocol + WorkbenchController
7. AIKS Bridge Plugin + SiyuanAdapter
8. Knowledge/Session workspace modes
9. Knowledge ↔ Session block references
10. Search/block positioning
11. Cache + embedding reindex
12. Database/Graph integration
13. fallback / diagnostics
14. full E2E migration and Windows validation
```

## 25. 最终产品定义

V4.1 完成后，AIKS 的产品模型为：

```text
Provider
   ↓
Raw Session
   ↓
SiYuan /10 AI Sessions
   ↓
AI Extraction
   ↓
SiYuan /20 Knowledge
   ↓
Embedded Workbench
   ↓
User Block Editing
   ↓
SiYuan Source of Truth
   ↓
AIKS Cache / Search / Embedding / Pipeline
```

最终边界：

> AIKS 是唯一产品入口与 AI 知识自动化系统；SiYuan 是内嵌的知识内容数据库、Block 编辑器和知识组织引擎。用户不需要在二者之间手工“发布”，因为用户看到和编辑的知识从一开始就已经位于 SiYuan 中。
