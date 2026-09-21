# AGENTS.md

本文件是 AI Coding Agent 和贡献者修改 AIKS 时应遵守的现行工程约束。历史设计文档用于理解演进过程；如果与本文件或当前代码冲突，以当前代码、测试和本文件为准。

## 1. 当前产品定位

AIKS V3 是一个本地优先、可观测、可追踪的 AI 工作知识处理系统。它读取 Claude Code、Codex、Gemini CLI、OpenCode、WorkBuddy 的本地 Session，将原始会话标准化、清洗、提炼为结构化知识，并可选生成向量用于混合检索；SiYuan 是可同步的知识出口之一，不是系统唯一的数据底座。

```text
Claude Code / Codex / Gemini CLI / OpenCode / WorkBuddy
                  ↓
          Session Provider Layer
                  ↓
          Canonical Session Model
                  ↓
       Parse / Clean / Pipeline Run
                  ↓
            AI Extraction
                  ↓
            KnowledgeItem
             ↙       ↘
     Text / FTS       Embedding
             ↘       ↙
          Hybrid Search
                  ↓
       Desktop / CLI / SiYuan
```

当前主实现位于：

- `crates/aiks-core`：Provider、状态库、Pipeline、知识、搜索、SiYuan 同步等核心逻辑。
- `apps/aiks-cli`：命令行入口。
- `apps/aiks-desktop`：React/Vite 前端与 Tauri 桌面端。
- `crates/aiks-core/migrations`：唯一正式 SQLite migration 链。

根目录不存在第二套 `src/` 或 `migrations/` 实现；不得重新建立与 workspace 重复的实现树。

## 2. 核心边界

### Provider

Provider 只负责发现和读取上游 Session，并输出 AIKS Canonical Model。

- Provider 不得直接写 SiYuan。
- Provider 不得直接创建 KnowledgeItem。
- Provider 不得把第三方内部数据结构泄漏到下游业务层。
- Claude/Codex/Gemini 的源文件必须只读。
- OpenCode SQLite 与 WorkBuddy SQLite 必须按只读/WAL-aware 边界访问，不得修改其 schema、WAL 或数据。
- WorkBuddy Provider 只可读取 `workbuddy.db` 会话元数据与 `projects/**/*.jsonl` transcript；不得读取或发布 `connectors/`、memory profile、MCP secrets、`.neodata_token` 或 `file-history/` 内容。

### 新增本地来源（PR #46）

Antigravity、Cursor、Cursor Agent、Cline、Roo Code、Kilo Code、GitHub Copilot、Kimi Code、Qwen Code、Continue、Aider 通过 `providers/native.rs` 与各格式模块接入。旧五个来源身份保持不变，来源显示和筛选统一消费 Core catalog；Share URL 缓存来源仍与本地目录来源区分。

- 新来源只经 `ScopedReader` 读取允许的 transcript 和必要元数据；加载时复查来源身份及路径边界，拒绝逃逸链接/reparse points，并保留读取预算和完整性诊断。
- Cursor 与 Kilo 索引 SQLite 只读且读取 WAL；不得枚举与会话无关的配置值或凭据。
- 部分扫描保留有效会话，但不能据此将未扫描/损坏/禁用来源的旧记录标为缺失。
- Antigravity IDE 的 usage/token 缓存不是对话，不得用它伪造消息。Aider 不默认扫描用户主目录或整个磁盘。
- Provider tests 必须显式配置 AI/Embedding 或使用回环测试服务；不得依赖部署模型默认值，也不得为了测试通过修改部署配置。
- `multi_provider_acceptance` 对 11 个适配器分别运行实际同步、去重、提炼、发布与混合搜索；真实安装环境和版本兼容范围仍单独验收。参见 `docs/reference-analysis/multi-provider-support.md`。

### State / Pipeline

SQLite 是本地状态和持久任务的事实源。`pipeline_job` 是持久队列，内存 channel 只能承担有界唤醒等辅助职责，不得恢复成无界 payload backlog。

Pipeline 状态必须真实反映执行结果：

- 配置为必需的阶段失败后，不得继续写成功状态。
- Embedding 已启用并配置时，分块、Embedding、索引失败不得进入 `READY`。
- Embedding 明确关闭/未配置时可以记录 `SKIPPED`。
- Job 必须具备可恢复 lease、持久 attempt/retry 和终态失败语义。
- 重启恢复不得依赖只存在于内存中的任务信息。

### Knowledge

`KnowledgeItem` 是 V3 的核心业务实体。

- 重新提炼时，只有安全、唯一的逻辑匹配才能复用已有 knowledge ID。
- 当前稳定身份策略以规范化后的 `(category, title)` 唯一匹配为准；不要根据 category 等弱条件猜测 rename。
- 被移除的已同步知识必须保留可追踪的 `REMOVED` sink tombstone，不能静默丢失远端映射。
- Knowledge 内容变化时，应使旧 chunk / embedding / FTS 派生数据失效并重建。

### Search / Embedding

AIKS 已实现自己的可选 Embedding Pipeline 和混合搜索，这些能力不是禁止项。

- AI 与 Embedding 对公开/新安装默认应保持 opt-in；不得恢复私网服务作为默认配置。
- 搜索错误不得用 `unwrap_or_default()` 等方式伪装成“0 条结果”。
- FTS 不可用或执行失败时应显式 degraded，并安全回退到参数化文本搜索。
- Vector 服务不可用时应保留文本搜索结果并报告 degradation。
- 普通查询不得无界加载整库向量；当前向量 rerank 必须保持有界候选集，除非引入经过验证的 ANN/vector index 方案。

### SiYuan

SiYuan 是支持的 Knowledge Sink / Session Archive 目标。

- 只通过公开 HTTP API 集成，不得直接修改 SiYuan SQLite 或 `.sy` 文件。
- 用户在 SiYuan 中的修改不得被默认静默覆盖；保持 conflict/baseline 语义。
- 并发 Knowledge→SiYuan 写入必须避免为同一个 knowledge item 创建重复远端文档。
- 慢 SiYuan 网络 I/O 不得占用 raw/session sync 的全局锁。

## 3. Provider 格式变化的排查顺序

上游格式无法解析时，不得凭感觉猜 schema。优先：

1. 查看已有匿名 fixture 和 parser tests。
2. 查看 AIKS 当前 Provider 实现与 `docs/reference-analysis/`。
3. 核对对应官方上游项目当前格式/代码。
4. 必要时参考 AICoder Session Viewer、CC Switch、ccusage 等既有实现。
5. 添加匿名 fixture 覆盖新格式。
6. 更新 Parser，并在兼容语义变化时提升 `parser_version`。
7. 跑相关 parser/golden tests 和完整 core 回归。

复制或修改第三方代码时必须遵守对应 License，并同步 `THIRD_PARTY_NOTICES.md`。

## 4. 并发与可靠性约束

- 不要在网络 I/O 周围持有与无关流程共享的粗粒度全局锁。
- 同一 durable queue 活跃周期内可以缓存 Provider discovery snapshot，但队列真正 idle 后必须允许刷新，避免永久陈旧。
- Extraction candidate 必须携带 canonical `source_session.id`，并保留 source/external ID 用于一致性校验；不得仅凭 `external_session_id` 二次猜测 Session。
- 同一逻辑任务的重复提交必须去重，过期 generation 不得形成无界积压。
- 单 Session 的损坏应尽可能隔离；不要因为一个坏样本让整个 Provider 扫描崩溃。

## 5. 数据库与 migration

正式 migration 只放在 `crates/aiks-core/migrations/`。

- 已发布/已使用的 migration 不应为了方便而原地改写；schema 变化优先追加新 migration。
- 数据迁移必须考虑已有用户数据库，不只验证空库。
- 新状态字段要明确默认值、回填和索引策略。
- destructive reset 与普通 rebuild 必须保持清晰区分。

## 6. 安全与公开仓库卫生

- 不得提交真实 API Key、Bearer Token、密码、AccessKey/SecretKey 等凭据。
- 不得把公司/家庭私网地址、内部模型服务地址作为默认值或示例值重新提交到公开仓库。
- `.workbuddy/` 等本地工作记忆不得纳入版本控制。
- 日志不得输出完整 Secret。
- 示例配置使用 loopback、空值或明确的占位符。

## 7. 修改原则

- 优先修改 canonical implementation，而不是复制一份新实现。
- 不要为了修一个局部问题顺带进行全仓格式化或无关重构。
- 对行为修复先写能复现问题的回归测试，再做最小实现修改。
- 新 API 优先保持现有调用方兼容；必须破坏兼容时，在同一变更里更新 CLI/Desktop/测试。
- Desktop 搜索等公共能力应复用 Core，不要在 Tauri command 中再维护另一套业务实现。
- 历史文档可以保留历史结论，但 README、AGENTS、TODO 必须描述当前状态。

## 8. 验证要求

按改动范围选择最小但充分的验证；重大 Core 变更至少应运行：

```bash
cargo test -p aiks-core
cargo check -p aiks-cli
```

涉及整个 workspace/Tauri 时应运行：

```bash
cargo test --workspace
```

涉及前端时：

```bash
cd apps/aiks-desktop
npm ci
npm run build
```

Tauri 在 Linux CI 中还需要 WebKit/AppIndicator 等系统依赖；`tauri.conf.json` 当前引用 `resources/siyuan/**/*`，纯 CI checkout 若未提供运行时资源，需要在验证环境创建占位目录，而不是为通过 CI 随意修改产品配置。

不得在未看到实际测试/构建结果时宣称修复完成。

## 9. 当前范围与路线图

当前代码已经包含 Provider、Canonical Model、SQLite 状态、增量同步、持久 Pipeline、AI Knowledge Extraction、Embedding、混合搜索、CLI、Tauri/React Desktop 和 SiYuan 同步。

当前工程化与后续工作以 GitHub Issues 和 `TODO.md` 为准。不要再按照旧 V1 Phase 清单重复实现已经存在的功能。
