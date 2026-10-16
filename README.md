# AI Knowledge Sync (AIKS)

[**简体中文**](README.md) · [**English**](README.en.md) · [Releases](https://github.com/Sunan869/aiks/releases) · [第三方致谢](THIRD_PARTY_NOTICES.md)

> **本地优先的 AI 工作会话与知识管理桌面应用。** 从多种 AI 编程助手的历史对话与公开分享链接中整理知识，支持可追踪的提炼流水线、知识工作台、全文/语义检索与 SiYuan 同步。

## 为什么需要 AIKS？

我们已经习惯让 AI 协助排查故障、讨论架构、编写代码和完善方案。但这些对话分散在不同的助手和工作区里：**问题解决了，结论却往往留在历史记录中**。过几周遇到类似问题，又要重新寻找上下文、重新解释、重新推理。

AIKS 解决的不只是“把聊天记录集中起来”，而是把 AI 协作过程变成一套可以持续复用的个人知识资产：

- **找回过程**：从多个 AI 工具只读采集 Session，不需要逐条复制粘贴。
- **提炼结果**：把冗长对话中的问题背景、关键步骤、解决方案整理为结构化知识，而不只保存摘要。
- **连接上下文**：知识能够关联来源会话、项目和文档，必要时追溯原始讨论。
- **在下一次工作中使用**：通过关键词、可选的向量语义检索和知识问答，复用过去解决过的问题。
- **始终由用户掌控**：本地优先、处理过程可观察，错误知识可管理；作废时清理 AIKS 管理的知识和索引，保留以后重新导入的可能。

> **一句话：从“AI 帮我完成一次任务”，走向“每次与 AI 协作都在积累下次可用的经验”。**

## 值得关注的核心亮点

### 01 · 多种 AI 工具，一个工作记录入口

不同助手的数据格式各不相同。AIKS 通过 Provider 适配器统一为 Canonical Session，支持本地历史记录与部分公开分享对话导入。可以在**工作记录**中按来源浏览、预览、筛选并定位原始上下文，不需要重新打开各个工具逐条翻找。


### 02 · 不止归档：自动把长对话提炼成知识

采集只是第一步。AIKS 的持久化流水线会按步骤进行解析、清洗、切分、AI 提炼以及可选的向量索引构建；知识条目可以包含标题、分类、摘要、正文和标签。**大段技术排查过程最终可以变成可检索、可管理的解决方案。**

任务可以查看阶段与异常状态，失败后按既有处理机制重试，而不是让用户猜测后台是否还在运行。


### 03 · 从关键词搜索到语义发现

忘了当时的文件名或具体术语，也不必只靠翻聊天记录：AIKS 同时提供全文检索与可选的 Embedding 语义检索，并在知识工作台与“问 AIKS”中复用已有内容。模型或向量服务不可用时，文本检索仍然可以使用。


### 04 · 个人知识库可管理，也可追溯

自动提炼的内容不是“一生成就永远正确”。可以通过分类、标签、项目关联、人工反馈和知识关系持续整理；还可以利用 SiYuan 查看与管理文档，并通过同步映射与哈希校验保护人工修改的内容。

**作废与恢复**不是简单隐藏搜索结果：作废会隔离自动处理，并对属于 AIKS 管理范围且通过安全校验的文档、知识和索引执行清理；保留来源身份，以便日后人工决定是否重新扫描和提取。遇到归属或内容冲突会暂停清理。


### 05 · 本地优先、过程透明

AIKS 将状态、知识和检索数据维护在本地 SQLite，来源文件按只读方式处理；支持手动同步与周期扫描。AI 提炼和向量服务可按需配置，默认不开启。用户可以从数据源、处理中心一路追踪到知识结果。

AIKS 当前主要面向**个人 AI 协作知识沉淀**，不是已经具备企业级权限管理和多人协同的知识平台。

## 它能用在哪些场景？

| 场景 | 典型做法 | 得到什么 |
| --- | --- | --- |
| 研发故障排查 | 将一次跨多轮的报错定位过程从 Session 提炼出来 | 下次快速找到触发条件、定位命令和解决步骤 |
| 技术方案与架构演进 | 归档不同工具中的方案讨论，并按项目组织知识 | 复用设计依据与历史决策，而非只留下最终代码 |
| 测试与质量工作 | 整理测试脚本、问题复现、修复分析等 AI 对话 | 形成可检索的案例与检查思路 |
| 个人学习与调研 | 导入有价值的 AI 研究对话或公开分享记录 | 沉淀概念、结论和参考上下文 |
| 跨工具持续工作 | 在不同 AI 助手中继续同类任务 | 从统一的知识入口检索过往经验 |

**典型使用路径：** 启用数据源 → 扫描历史会话 → 在处理中心观察提炼 → 查看结构化知识 → 搜索并复用 → 按需修订或作废。

## 功能概览

| 能力 | 说明 |
| --- | --- |
| 多源工作记录 | 读取 Claude Code、Codex CLI、Gemini CLI、OpenCode、WorkBuddy，以及 Antigravity、Cursor、Cursor Agent、Cline、Roo Code、Kilo Code、GitHub Copilot、Kimi Code、Qwen Code、Continue、Aider 的本地 Session；另支持部分 AI 对话的**公开分享链接**导入 |
| 本地优先 | Provider 源文件按只读方式读取，使用 SQLite 保存会话标识、任务状态、知识和检索数据；个人离线模式不要求使用云端账户 |
| 可靠知识提炼 | 可配置 OpenAI-compatible 模型；持久化任务队列、流水线阶段、失败重试、崩溃恢复、任务诊断与人工核查 |
| 知识工作台 | 结构化知识条目、项目关联、分类与标签、归档、人工反馈和知识关系 |
| 检索与问答 | 全文检索（FTS）、可选 Embedding/语义混合检索；向量服务不可用时支持文本检索降级 |
| SiYuan 集成 | 内嵌 SiYuan Kernel，支持会话原文与知识文档同步、冲突检测、内容哈希保护；不直接修改 SiYuan 内部数据库 |
| 处理中心 | 统一任务列表、全库搜索与分页、状态筛选、阶段预览及独立口径的会话/流水线统计 |
| 作废与恢复 | 作废会话时先隔离检索与自动任务，再安全清理 AIKS 管理的 SiYuan 文档、知识及索引；保留来源标识，手动恢复后重新扫描和提取 |

**支持边界：** 不保证覆盖上述工具的所有历史/未来会话格式。不同 Provider 的格式支持、路径和已知限制见 [多来源兼容说明](docs/reference-analysis/multi-provider-support.md)。公开分享页面格式随平台更新可能改变，且仅导入用户有权访问的内容。

## 工作流程

```text
AI 编程助手本地 Session / 公开分享对话
                   │ 只读解析
                   ▼
           Canonical Session
                   ▼
      SQLite 状态与持久化处理队列
                   ▼
    清洗 / 分块 / AI 提炼 / 人工复核
                   ▼
             KnowledgeItem
                │      │
               FTS   Embedding（可选）
                └──┬───┘
                   ▼
        搜索 / 知识工作台 / 问 AIKS
                   │
                   ▼
           SiYuan 文档同步（可选）
```

SiYuan 是 AIKS 使用的文档引擎与知识出口之一；AIKS 的会话状态、提炼知识和检索索引由自身维护。**默认不覆盖用户人工修改过的 SiYuan 文档。**

## 下载和运行

从 [GitHub Releases](https://github.com/Sunan869/aiks/releases) 获取 Windows、macOS 或 Linux 对应安装包。安装包版本和可用平台以实际 Release 资产为准。

### macOS 安装与首次运行

1. 从 [Releases](https://github.com/Sunan869/aiks/releases) 下载与你的 Mac 芯片架构匹配的 `.dmg`：Apple Silicon（M1/M2/M3/M4 等）选择 `aarch64/arm64`，Intel Mac 选择 `x64`。
2. 双击打开 DMG，将 **AIKS.app** 拖到「应用程序（Applications）」文件夹。
3. 在 Finder 的「应用程序」中启动 AIKS。

**关于未签名安装包：** 当前发布包尚未完成 Apple Developer ID 签名和 Apple 公证，macOS 可能提示“无法验证 AIKS 是否包含恶意软件”或拦截首次启动。请先确认文件来自上述 GitHub Release、版本与架构匹配，并在 macOS「系统设置 → 隐私与安全性」中查看是否提供针对 AIKS 的「仍要打开（Open Anyway）」选项。

若系统仍阻止启动，且**你确认该安装包可信并愿意承担绕过校验的风险**，可在终端仅针对 AIKS.app 执行：

```bash
sudo xattr -dr com.apple.quarantine /Applications/AIKS.app
open /Applications/AIKS.app
```

说明：`xattr` 命令会移除应用的下载隔离标记，**不会**提供开发者签名或公证；不要在未知来源的应用上使用，也不要执行全局关闭 Gatekeeper 的命令。如果应用安装在其他路径，请将 `/Applications/AIKS.app` 替换成实际路径。若仍失败，请记录 macOS 版本、芯片架构及错误提示并提交 Issue。

### Windows / Linux

Windows 下载与系统架构匹配的 `.exe` 安装程序或 `.msi`；Linux 使用对应架构的 `.deb` 或 `.AppImage`。Linux AppImage 首次运行通常需赋予执行权限（`chmod +x <文件名>.AppImage`）。

初次运行建议：
1. 在「数据源」页面检查 Provider 及扫描目录，仅启用需要的来源。
2. 在「设置」中配置 AI 模型与可选 Embedding 服务；**AI/Embedding 默认关闭**，未配置也可以浏览已导入内容。
3. 运行扫描与同步，在「处理中心」查看流水线状态，在「工作记录」预览会话，并到「知识」和搜索中查看提炼结果。
4. 先用单独测试数据目录验证 SiYuan 同步与作废；作废是**可能删除 AIKS 管理的远端文档**的操作，发生归属/内容冲突会停止清理并保留恢复检查记录。

**作废不是删除 Provider 原始会话文件。** 处于作废状态的来源不会被自动重新导入知识库；恢复将解除隔离并重新走来源扫描、同步和提炼。部分 Web 分享来源在恢复时需要再次导入原始分享链接。

## 从源码开发

要求：Rust stable、Node.js 22+、npm，以及对应平台的 [Tauri 2 开发依赖](https://v2.tauri.app/start/prerequisites/)。

```bash
# Rust workspace
cargo build --workspace
cargo test --workspace

# Desktop UI
cd apps/aiks-desktop
npm ci
npm run build
npm run tauri dev
```

Windows PowerShell 下也可使用仓库的启动脚本：

```powershell
$env:AIKS_DATA_DIR = "E:\\AIKS-Test\\data"
Remove-Item Env:AIKS_BACKEND_MODE -ErrorAction SilentlyContinue
.\\scripts\\dev.ps1
```

**开发环境提醒：** 如果已运行的 `SiYuan-Kernel.exe` 占用 `target/debug/resources`，应先正常退出旧的 AIKS/SiYuan 进程再重新构建。切勿通过删除正式数据目录解决构建文件占用问题。

主要目录：`crates/aiks-core/`（Rust 核心、Provider、SQLite、Pipeline）、`apps/aiks-desktop/`（React/Vite/Tauri）、`apps/aiks-cli/`（CLI）、`docs/`（设计和兼容资料）。配置示例：`config.example.toml`；安全与工程约束：`AGENTS.md`。

CLI 示例：

```bash
cargo run -p aiks-cli -- --help
cargo run -p aiks-cli -- --config ./config.toml doctor
```

请勿提交包含 API Key、Token、个人对话内容或内部路径的日志与测试数据。

## 致谢与第三方项目

AIKS 受许多优秀项目启发，部分会话与分享解析能力也参考并改编了相关项目的实现。特别感谢：

- [SiYuan / 思源笔记](https://github.com/siyuan-note/siyuan) — 内嵌知识文档引擎和 HTTP API。
- [AICoder Session Viewer](https://github.com/seastart/aicoder-session-viewer) — Claude/Codex/Gemini/OpenCode 会话解析的早期重要参考与改编来源。
- [CC Switch](https://github.com/farion1231/cc-switch)、[ccusage](https://github.com/ccusage/ccusage) — 数据源发现、会话扫描及使用信息处理的参考。
- [OpenAI Codex](https://github.com/openai/codex)、[Gemini CLI](https://github.com/google-gemini/gemini-cli)、[OpenCode](https://github.com/anomalyco/opencode)、[Claude Code](https://github.com/anthropics/claude-code) — Provider 格式及行为兼容参考；引用并不代表项目背书。
- [chatgpt-share-parser](https://github.com/evanhu1/chatgpt-share-parser)、[chat-share-reader](https://github.com/pencil311/chat-share-reader) 等 — 公开分享对话格式研究及解析参考；详细项目来源和改编说明见 [第三方致谢](THIRD_PARTY_NOTICES.md)。
- [Tauri](https://github.com/tauri-apps/tauri)、[React](https://github.com/facebook/react)、[Radix UI](https://github.com/radix-ui/primitives) 及 Rust 社区 — 桌面应用和 UI 基础设施。

上述项目商标和版权属于各自所有者；AIKS 与这些项目不存在当然的官方隶属或认可关系。**详细项目来源、固定版本及参考/改编范围：** [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)。

## 贡献与免责声明

- 欢迎提交 Issue、使用反馈和改进建议；贡献代码前请阅读 `AGENTS.md`。
- 软件按现状提供，不承诺一定兼容所有 Provider 版本，也不保证提炼结果完全准确。执行作废、重建或升级前请备份重要资料。

---
如果你使用了 AIKS，欢迎反馈可重现的问题和兼容性信息。感谢每一位上游作者与贡献者。
