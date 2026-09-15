# AIKS V4 Native Knowledge Workbench 设计说明

## 1. 背景

AIKS V3 已经具备完整的本地知识处理链路：Session Provider、Canonical Session、持久化 Pipeline、AI Knowledge Extraction、KnowledgeItem、Embedding、Hybrid Search、Desktop、CLI 与 SiYuan 同步。

当前产品体验仍存在明显割裂：

1. Desktop 与 SiYuan 在交互上像两个独立产品，用户在 AIKS 中完成提炼后仍需要跳到 SiYuan 才能继续管理知识。
2. `KnowledgeItem` 当前强制绑定 `source_session_id`，知识天然被建模为“AI 会话提炼结果”，无法把手动知识作为一等公民。
3. Desktop 已有知识列表和详情，但详情只读；“同步到 SiYuan”仍处于知识页主要操作位。
4. V2 遗留的嵌入式 SiYuan runtime、独立知识窗口和运行时资源继续增加打包、CI 和产品边界复杂度，而 AIKS 实际只需要 SiYuan 的知识发布能力。

V4 的目标不是重新实现一个完整笔记软件，而是把 AIKS 明确变成一个 **AI 原生、本地优先的知识工作台**：知识既可以自动从 AI 会话中产生，也可以由用户主动创建和维护；SiYuan 只保留为可选 Publisher。

---

## 2. 产品定位

AIKS V4 定义为：

> 面向 AI Coding / AI 对话场景的本地优先知识沉淀、维护、检索与复用系统。

核心闭环：

```text
AI Session / 手动录入
          ↓
      KnowledgeItem
          ↓
  编辑 / 整理 / 搜索 / 关联
          ↓
       本地知识库
          ↓
   可选发布到外部系统
          ↓
        SiYuan
```

SiYuan 不再是主知识界面，不再决定 AIKS 能否创建、编辑、浏览或搜索知识。

---

## 3. 设计原则

### 3.1 AIKS Native First

以下能力必须完全由 AIKS 本地提供：

- KnowledgeItem 的创建、编辑、归档与收藏。
- 手动知识录入。
- AI 会话知识提炼。
- 来源追溯。
- 分类、标签、项目字段。
- FTS / Hybrid Search。
- Knowledge Detail / Markdown 编辑。
- 派生 chunk / embedding 生命周期。

### 3.2 SiYuan Optional

SiYuan 仅作为外部 Publisher：

- 不作为 AIKS 的主存储。
- 不作为 AIKS 的主编辑器。
- 不在主知识页占据一级操作位。
- 不要求 AIKS 启动时自动拉起完整 SiYuan runtime。
- 保留 HTTP API 发布、远端 mapping、baseline 与 conflict 保护。

### 3.3 KnowledgeItem 单一模型

自动提炼、手动创建最终都进入同一 `knowledge_item` 表和同一搜索/编辑/UI 流程，不建立“自动知识表”和“手动笔记表”两套模型。

### 3.4 用户编辑优先

AI 提炼出来的 KnowledgeItem 一旦被用户手动编辑，后续 Session 重新提炼不得静默覆盖用户修改，也不得因为提炼结果变化而删除该条知识。

### 3.5 不实现完整 SiYuan

V4 不实现：

- Block database。
- 白板。
- 闪卡。
- 数据库视图。
- PDF 标注。
- 完整双链笔记系统。
- SiYuan Web UI 嵌入。

V4 编辑器只覆盖知识管理真正需要的 Markdown 能力。

---

## 4. 方案比较

### 方案 A：继续以 SiYuan 为主知识编辑器

优点：开发量小，继续利用 SiYuan 完整编辑能力。

缺点：产品仍然割裂；AIKS 无法成为独立知识系统；手动录入仍依赖外部应用；后续任何知识能力都需要协调两个状态模型。

不采用。

### 方案 B：AIKS 原生知识库，SiYuan 保留为可选 Publisher

优点：数据模型统一、产品体验连续、手动知识自然、搜索和 AI 能力都围绕同一 KnowledgeItem 演进；SiYuan 仍可作为用户已有知识生态出口。

缺点：需要一次数据库 migration、Knowledge CRUD、编辑器与 Desktop IA 调整。

**采用。**

### 方案 C：AIKS 复制 SiYuan 全套笔记能力

优点：理论上功能最完整。

缺点：严重超出 AIKS 核心价值，维护成本高，重复造轮子。

不采用。

---

## 5. V4 领域模型

### 5.1 KnowledgeItem 来源

V4 引入 `source_type`：

```text
conversation
manual
```

未来可以追加 `file` / `external`，但 V4 不提前实现导入系统。

`source_session_id` 由 `NOT NULL` 改为 nullable：

- `conversation`：必须有 `source_session_id`。
- `manual`：`source_session_id = NULL`。

### 5.2 管理权

新增 `managed_by`：

```text
pipeline
user
```

规则：

- AI 自动提炼：`source_type=conversation, managed_by=pipeline`。
- 手动创建：`source_type=manual, managed_by=user`。
- 用户编辑 AI 知识：保留 `source_type=conversation` 与来源 Session，但将 `managed_by=user`。

这样“来源”与“谁可以自动覆盖它”是两个独立概念。

### 5.3 用户状态

新增：

```text
is_favorite INTEGER NOT NULL DEFAULT 0
status TEXT NOT NULL DEFAULT 'active'
```

`status` V4 仅支持：

```text
active
archived
```

V4 默认使用归档而不是物理删除，避免误删 Knowledge→SiYuan mapping 和用户手工整理结果。

### 5.4 推荐 migration 后的核心字段

```text
knowledge_item
- id TEXT PRIMARY KEY
- source_session_id INTEGER NULL
- source_type TEXT NOT NULL DEFAULT 'conversation'
- managed_by TEXT NOT NULL DEFAULT 'pipeline'
- status TEXT NOT NULL DEFAULT 'active'
- is_favorite INTEGER NOT NULL DEFAULT 0
- project_name TEXT NULL
- title TEXT NOT NULL
- category TEXT NOT NULL
- summary TEXT NOT NULL
- content TEXT NOT NULL
- tags TEXT NOT NULL
- confidence REAL NOT NULL
- worth_extracting INTEGER NOT NULL
- created_at TEXT NOT NULL
- updated_at TEXT NOT NULL
```

migration 必须保留现有所有 KnowledgeItem、chunk、embedding、FTS 和 sync mapping。

---

## 6. Pipeline 与用户编辑保护

当前 `save_items(session_id, ...)` 会按 Session 重建提炼结果。V4 调整为：

1. `managed_by=pipeline` 的旧知识继续参与正常 reconciliation。
2. `managed_by=user` 且属于同一 Session 的知识不得被 pipeline 删除。
3. 如果新提炼结果与一个 `managed_by=user` KnowledgeItem 形成唯一 `(category,title)` 逻辑匹配：
   - 复用该 ID 作为已匹配项。
   - 不覆盖 title/summary/content/tags/category/project。
   - 不创建重复 KnowledgeItem。
4. 如果用户已经把标题改到无法与提炼结果匹配：
   - 该用户知识继续保留。
   - pipeline 可以产生新的自动知识条目。
   - 不使用 category-only 等弱匹配猜测身份。
5. 手动知识永远不参与 Session pipeline reconciliation。

这保证“AI 自动更新”不会破坏用户主动维护的知识。

---

## 7. Knowledge CRUD 服务

在 Core 中建立正式的 Knowledge 编辑服务，不把 CRUD SQL 分散在 Tauri command。

推荐接口：

```rust
pub struct CreateKnowledgeInput {
    pub title: String,
    pub category: KnowledgeCategory,
    pub summary: String,
    pub content: String,
    pub tags: Vec<String>,
    pub project_name: Option<String>,
}

pub struct UpdateKnowledgeInput {
    pub title: String,
    pub category: KnowledgeCategory,
    pub summary: String,
    pub content: String,
    pub tags: Vec<String>,
    pub project_name: Option<String>,
}

KnowledgeService::create_manual(input)
KnowledgeService::update(id, input)
KnowledgeService::set_favorite(id, bool)
KnowledgeService::archive(id)
KnowledgeService::restore(id)
KnowledgeService::get(id)
KnowledgeService::list(filter)
```

所有写操作必须事务化维护 FTS。

---

## 8. Chunk / Embedding 生命周期

任何知识正文变化都必须避免旧向量继续代表旧内容。

规则：

1. title / summary / content / tags 更新后同步刷新 FTS。
2. 内容更新后删除该 KnowledgeItem 现有 `knowledge_chunk` 和 `embedding_record`。
3. 抽取一个单 KnowledgeItem 可复用的索引能力，使自动提炼和手动编辑共享同一派生数据路径。
4. Embedding 未启用时：只维护 FTS，不视为错误。
5. Embedding 已启用时：保存知识成功与向量刷新解耦；向量刷新失败不回滚用户已保存正文，但 UI/日志需要报告“文本已保存，向量索引刷新失败”。
6. 普通搜索继续遵守 V3 的 bounded candidate 与 degraded 语义。

V4 不引入新的 ANN 引擎。

---

## 9. Desktop 信息架构

### 9.1 一级导航

保留：

```text
概览
会话
知识库
搜索
处理中心
数据源
设置
```

不增加独立“SiYuan 知识库”一级导航。

### 9.2 知识库页

顶部主操作：

```text
[ + 新建知识 ]
```

筛选：

```text
全部
AI 提炼
手动创建
收藏
已归档
```

附加筛选：分类、项目。

列表项展示：

- 标题。
- 摘要。
- 分类。
- 标签。
- 来源类型。
- 收藏状态。
- 更新时间。

SiYuan 批量同步不再作为知识库页主要按钮。

### 9.3 Knowledge Detail

头部操作：

```text
编辑
收藏/取消收藏
归档/恢复
更多 → 发布到 SiYuan
```

来源卡：

conversation：

```text
来源：OpenCode / Codex / Claude / Gemini
Session title
[查看原始工作记录]
```

manual：

```text
来源：手动创建
创建时间
```

### 9.4 Knowledge Editor

创建和编辑共用同一组件。

字段：

- 标题（必填）。
- 分类。
- 项目（可选）。
- 标签。
- 摘要。
- Markdown 正文（必填）。

编辑器采用轻量 Markdown 输入 + Preview，不实现完整 Block Editor。

V4 可以引入 `react-markdown` + `remark-gfm` 用于只读 Preview；编辑仍由 textarea 完成，避免一次引入大型编辑框架。

保存验证：

- title trim 后不能为空。
- content trim 后不能为空。
- tags 去空、trim、去重。
- summary 可以为空；为空时不自动调用 LLM，避免“手动保存必须依赖 AI”。

---

## 10. SiYuan Integration / Publisher

### 10.1 产品边界

SiYuan 从“知识模块”降为“外部集成”。

设置页新增或明确：

```text
外部集成
└─ SiYuan
   - Endpoint
   - Token
   - Notebook
   - Root Path
   - Test Connection
   - Publish All Active Knowledge
```

知识详情可发布单条。

### 10.2 代码边界

引入 Publisher 抽象：

```rust
#[async_trait]
pub trait KnowledgePublisher {
    async fn publish(&self, knowledge_id: &str) -> anyhow::Result<PublishResult>;
}
```

V4 只实现 `SiyuanKnowledgePublisher`。

现有 `knowledge_sync_target`、baseline、conflict、REMOVED tombstone 继续沿用，不重新设计远端映射协议。

### 10.3 嵌入式 SiYuan runtime

V4 默认产品路径删除：

- 自动启动/重启完整 SiYuan runtime。
- `open_knowledge_window` 弹出 SiYuan Web UI 的主流程。
- Tauri 对 `resources/siyuan/**/*` 的强依赖。
- 为嵌入式 SiYuan 准备独立窗口作为知识入口。

保留外部 SiYuan HTTP API client。

如果当前安装已有嵌入式运行时数据，不主动删除用户目录，只停止把它作为 V4 必需组件。

---

## 11. Tauri API

V4 Desktop API 增加：

```text
create_knowledge
update_knowledge
set_knowledge_favorite
archive_knowledge
restore_knowledge
publish_knowledge
```

扩展：

```text
list_knowledge
get_knowledge_detail
```

返回字段加入：

```text
source_type
managed_by
status
is_favorite
session_id: nullable
source/session metadata: nullable
```

Tauri command 只负责参数/DTO 适配，业务逻辑调用 Core `KnowledgeService`。

---

## 12. Search 行为

V4 搜索默认：

- 只搜索 `status=active`。
- 同时搜索 conversation 和 manual KnowledgeItem。
- Favorite 不改变 ranking，仅作为筛选条件。
- archived 知识只在显式“已归档”视图中出现，不进入普通全文/语义搜索。

手动知识保存后立即能通过 FTS 搜到。

---

## 13. 兼容性

### 13.1 Existing DB

V4 migration 必须从现有 V3 数据无损升级：

```text
source_type = conversation
managed_by = pipeline
status = active
is_favorite = 0
```

现有 `source_session_id` 值全部保留。

### 13.2 Existing SiYuan mapping

现有 `knowledge_sync_target` 不清空、不重建 ID；原有远端文档继续可更新。

### 13.3 CLI

现有 CLI `sync-knowledge` 暂时保留兼容，不在 V4 中强制破坏命令名；文档语义改为“publish to SiYuan”。

---

## 14. 测试策略

### Core regression

必须覆盖：

1. V3 DB migration 到 V4 后旧知识数据完整。
2. 手动 KnowledgeItem 可以没有 `source_session_id`。
3. 手动创建会写 FTS。
4. 编辑后 FTS 内容更新。
5. 编辑正文后旧 chunk/embedding 失效。
6. 用户编辑 AI 知识后 `managed_by=user`。
7. Session re-extraction 不覆盖 user-managed 内容。
8. Session re-extraction 不删除 user-managed 内容。
9. archive 后普通 search 不返回。
10. restore 后重新可检索。
11. favorite 状态持久化。
12. 现有 SiYuan mapping 不因手动编辑而丢失。

### Desktop

至少覆盖：

1. API client 的 create/update/archive/favorite/publish 映射。
2. Knowledge filter 逻辑。
3. Editor 表单校验与 tag normalize。
4. Manual source 与 conversation source 的显示分支。

### Full validation

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd apps/aiks-desktop
npm test
npm run build
```

---

## 15. 实施顺序

V4 实现顺序固定为：

1. V4 migration + domain model。
2. Core KnowledgeService CRUD + user-managed reconciliation protection。
3. FTS / chunk / embedding 派生数据生命周期。
4. Tauri DTO / commands。
5. Desktop API types/client/mock。
6. Knowledge Workbench list/filter/create。
7. Knowledge Editor + detail actions。
8. SiYuan Publisher 化 + 移出主知识流程。
9. 移除嵌入式 SiYuan runtime 默认依赖。
10. README / AGENTS / TODO 更新。
11. 全量 CI、自审、PR。

每个行为变化先写 RED regression，再实现 GREEN。

---

## 16. V4 完成标准

V4 只有在以下全部成立时才算完成：

- 用户可以在没有任何 Session 的情况下手动创建 KnowledgeItem。
- 手动知识与 AI 知识使用同一列表、详情、搜索和编辑能力。
- AI 提炼知识可以被用户编辑，且重新提炼不会静默覆盖用户版本。
- 普通知识管理不依赖 SiYuan 安装或运行。
- SiYuan 只作为设置中的可选外部 Publisher 和知识详情发布动作存在。
- 默认 Desktop 不需要完整嵌入式 SiYuan runtime 才能构建和运行。
- Existing V3 SQLite 可无损升级。
- Existing SiYuan mapping 不丢失。
- Rust workspace 与 Desktop tests/build 全绿。
- README / AGENTS / TODO 明确描述 V4，而不是旧 V3 产品边界。

---

## 17. 明确不进入 V4 的范围

以下放到后续 V4.x / V5：

- 文件批量导入。
- Notion / Obsidian / Confluence Publisher。
- 知识图谱。
- 自动重复知识合并。
- 知识过期检测。
- 自动把检索结果注入 Claude/Codex/OpenCode 新会话。
- 完整 WYSIWYG / Block Editor。
- sqlite-vec / ANN 引擎切换。

这些能力可以建立在 V4 的统一 KnowledgeItem 与 Publisher 边界之上，但不阻塞本次实现。