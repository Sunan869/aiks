# 多工具本地会话支持矩阵（PR #46）

日期：2026-09-21。范围：`feature/multi-provider-integration` 的 11 个新增本地来源。

这是已实现的文件布局与自动化验收范围，不是所有产品版本的通用兼容承诺。原有 Claude Code、Codex、Gemini CLI、OpenCode、WorkBuddy 五个本地来源保留。因此本地可配置来源为 16 个；加上 `main` 已有的三个 Share URL 缓存来源，Core catalog 共 19 个身份。本任务不重新开发 Share URL。

## 来源、目录与支持边界

以下路径都相对于配置的数据根目录。`User` 指编辑器用户数据根，不是应用程序安装目录。每个来源都有独立稳定 key 和 parser version。

| 显示名 / key | 默认或显式根 | 当前读取的会话布局 | 自动化样例及边界 |
| --- | --- | --- | --- |
| Antigravity / `antigravity` | `~/.gemini/antigravity-cli`、`~/.gemini/antigravity` | `brain/<id>/.system_generated/logs/transcript_full.jsonl`，不存在时读取 `transcript.jsonl` | CLI step/source/content 日志；保留真实对话与未知事件。只有 IDE token-monitor/usage 缓存时明确不支持，不伪造消息。 |
| Cursor / `cursor` | Cursor 的 `User` 目录 | `globalStorage/state.vscdb` 与 `workspaceStorage/<workspace>/state.vscdb` | Composer JSON、会话头引用的 bubble、旧 workspace Composer 列表，含 WAL-only 更新/标题变化；非 Composer 历史 schema 不承诺支持。 |
| Cursor Agent / `cursor_agent` | `~/.cursor` | `projects/<project>/agent-transcripts/**/*.jsonl`，有限层级 | JSONL 角色/内容及 user_query 文本；纯 TXT 或其它数据库版本不在本轮已验证格式内。 |
| Cline / `cline` | 编辑器 User 或扩展数据根 | `globalStorage/saoudrizwan.claude-dev/tasks/<task>/api_conversation_history.json`，回退 `ui_messages.json` | API 对话与 UI 文本/思考；必要任务元数据。显式扩展根从 `tasks/` 开始。 |
| Roo Code / `roo_code` | 编辑器 User 或扩展数据根 | 同上，扩展 ID 为 `rooveterinaryinc.roo-cline` | 独立来源、任务身份与 fixture；复用经过测试的 Cline 家族消息转换。 |
| Kilo Code / `kilo_code` | 编辑器 User 或扩展数据根 | 同上，扩展 ID 为 `kilocode.kilo-code` | 无 Cline 索引仍可发现任务；支持只读查询编辑器 ItemTable 中该扩展的任务索引，不读取其它配置值。 |
| GitHub Copilot / `github_copilot` | `~/.copilot`，Code / Code - Insiders 的 User | `session-state/<id>/events.jsonl`；VS Code `workspaceStorage/<workspace>/chatSessions/*.json` 或 `.jsonl` | CLI/Desktop 共用会话事件去重，workspace.yaml 必要元数据；VS Code 快照、set/append/delete 补丁。不是云端历史下载。 |
| Kimi Code / `kimi_code` | `~/.kimi-code`、`~/.kimi` | `sessions/<workspace>/<session>/agents/main/wire.jsonl`，旧版 `context.jsonl`，必要 `state.json` | 主 Agent 事件重放、clear/undo/compaction、工具调用及未完成工具标识；新旧格式独立测试。未承诺导入所有子 Agent 存储。 |
| Qwen Code / `qwen_code` | `~/.qwen` | `projects/<project>/chats/*.jsonl` | 内部 session identity、cwd、消息 parts、工具/推理块，受控 Unknown 保留。 |
| Continue / `continue` | `~/.continue` | `sessions/<id>.json` | `history[].message`；跳过 `sessions.json` 索引，不把 context 附件内容当成独立消息；标题修改和消息追加保持身份。 |
| Aider / `aider` | **必须指定项目根** | 项目范围内 `.aider.chat.history.md` | Markdown 会话头与消息，代码围栏内相似标题不会切出伪会话；追加不改变已存在会话身份。 |

首次使用可在“数据源 → 目录与开关”中保存根目录，每行一个；保存后重启。自动探测不到实际安装时应配置真实绝对路径，不使用字面量 `~`。旧五个来源继续采用原来的单 `path` 配置，不迁移历史 source key。

### 路径优先级

非空 `path` > 非空 `paths` > 支持的环境变量 > 平台默认根。显式根不可用会报告错误，不偷偷改用其它用户的数据。

编辑器 User 默认目录：Windows `%APPDATA%/<editor>/User`；macOS `~/Library/Application Support/<editor>/User`；Linux `${XDG_CONFIG_HOME:-~/.config}/<editor>/User`。Cline 家族探测 Code、Code - Insiders、Cursor、VSCodium、Codium；VS Code Copilot 探测 Code 和 Code - Insiders。自定义/便携/远程 profile 可用显式路径；不自动启动 WSL/SSH 或枚举所有系统用户。

Qwen 环境变量为 `QWEN_RUNTIME_DIR` / `QWEN_HOME`；Continue 为 `CONTINUE_GLOBAL_DIR`；Cursor 为 `CURSOR_USER_DIR`；Copilot 为 `COPILOT_CLI_HOME`；Kimi 为 `KIMI_CODE_HOME` / `KIMI_SHARE_DIR` / `KIMI_HOME`。其它来源不猜测未经实现的环境变量。

### parser version

`qwen-jsonl-v1`、`continue-session-v1`、`cursor-agent-jsonl-v1`、`cline-task-v1`、`roo-task-v1`、`kilo-task-v1`、`aider-markdown-v1`、`kimi-session-v1`、`cursor-sqlite-v1`、`copilot-history-v1`、`antigravity-local-v1`。

## 自动化验证的实际路径

`crates/aiks-core/tests/multi_provider_acceptance.rs` 中的 11 个案例分别使用真正的 Provider 工厂/解析器，不用 FakeProvider 代替本轮接入。共同 helper `tests/support/multi_provider_flow.rs` 执行：

1. 发现并加载合成多轮会话，经现有 SyncEngine 创建会话记录和原始文档。
2. 再次同步，验证同一数据库身份、无重复文档。
3. 经现有持久 PipelineWorker 清洗、调用回环 AI/Embedding 测试服务并产生知识；不访问真实账户或部署模型。
4. 用现有 publisher 发布知识，读取规范 Markdown 并建立现有 canonical index。
5. 用现有统一搜索入口分别检索会话和知识，验证 source 过滤和语义命中。

这证明样例格式能走通接入链路，**不是这些工具真实安装环境、真实模型质量或桌面点击操作均已验收**。

辅助测试：

- `multi_provider_variants`：不同根/同 ID 命名空间，Kilo 索引只读，Copilot Desktop 去重、VS Code 补丁，Cursor headers/WAL，Kimi 重放，Antigravity usage-only 拒绝。
- `provider_local_io`：路径、链接、JSONL 行/文件预算、BOM、无末尾换行、半写尾行、未 checkpoint 的 WAL。
- `provider_catalog`：19 个 catalog 身份唯一、旧 key 保持、只修改被选 Provider 配置，未知字段保留。
- `provider_missing_scope`：移除配置根、损坏/禁用来源不误标旧数据丢失，选定来源不扫描无关来源。
- `provider_platform_variants`：三种 Cline UI fallback，中文路径；Windows 分支实际创建 junction 并验证根/父目录拒绝及外部文件不变。
- `provider_runtime_regressions`：坏邻居不阻断好会话的持久处理，Cursor 损坏库不健康，Continue 元数据/追加更新与半写快照。

标准 `.github/workflows/ci.yml` 继续承担格式、workspace Clippy/tests、前端 tests/build、Windows 桌面编译和仓库检查。另有只读 `provider-contracts.yml` 在 Windows 真正执行上述测试，包括 junction 行为，而不只 cargo check。

开发阶段的写分支辅助脚本与写权限 workflow 已在收尾提交中移除；最终验收必须看 PR #46 **最新 HEAD** 的只读 CI，不能用辅助脚本应用前的 SHA 或旧的绿色作业代替。实际最新结果记录在 PR，本文不预填尚未结束的 CI 成功状态。

## 读取安全、身份和刷新

新适配器共用 ScopedReader，限制单行 8 MiB、单文件/指定数据库扫描负载 256 MiB、目录条目预算 100,000、递归深度最高 16，来源根最多 32 个。达到边界报告不完整，不静默当成全库扫描成功。SQLite 只读、有限 busy timeout、读取 WAL；不写上游设置或 transcript。

加载阶段再次验证 source 与允许路径。拒绝越界、符号链接及 Windows reparse points；这些是包含性校验，**不是针对恶意并发文件系统写入的竞态隔离保证**。不读取连接凭据，不跟随对话里提到的文件路径，不根据 cwd 扩大扫描。

损坏/半写会话不应覆盖上次完整快照；同一来源的其它有效会话继续处理。旧来源/已导入知识不会因本次关闭来源或删除一个配置根而被删除。确切缺失判定仍以完整扫描覆盖范围为准。

部分工具 ID 以规范存储目录命名空间化，保证多根不同会话不碰撞且根重排不变；移动真实历史文件/重命名存储根可能改变身份，未承诺跨路径内容猜测归并。修改来源配置后需重启。新来源使用现有手工/周期同步；没有声称本轮已为每个工具实现独立实时文件 watcher。

## 真实环境待验收

对实际使用的工具分别验证：数据根自动发现或手工覆盖、中文与附件提示、多轮/工具显示、重复同步、追加消息、仅修改标题、工具运行时 WAL/半写、重启保留、工作记录筛选、知识发布和搜索。记录安装版本与布局；提供问题样例前先匿名化，不上传凭据或整份私密工作目录。

Antigravity IDE 仅统计缓存仍明确不支持；发现新的可读真实消息布局后需要证据和 fixture 才能新增适配。其它表外布局同理，不凭工具名推断兼容。大规模真实库的首次扫描延迟和峰值内存尚需实测；读取预算不是性能 SLA。

## 参考和许可

主要参考 `jhlee0409/claude-code-history-viewer`，固定 commit `fdfc766ce7f0d76dceb03087aedac47add33d61b`；其 `src-tauri/src/providers/` 中对应工具的布局/消息转换为原生适配依据。Kimi 事件语义同时参考 `MoonshotAI/kimi-code` commit `6a214b85e53e58a9ef6480f27bcb7b0103c0e34e`。原有 seastart 参考继续保留。

本轮不是整体嵌入或分发参考查看器；UI、同步、存储与搜索仍使用 AIKS。许可归属及关联文件见根目录 `THIRD_PARTY_NOTICES.md` 的本地 Provider 条目。
