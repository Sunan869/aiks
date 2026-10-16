# AI Knowledge Sync (AIKS)

[**简体中文**](README.md) · [**English**](README.en.md) · [Releases](https://github.com/Sunan869/aiks/releases) · [第三方致谢](THIRD_PARTY_NOTICES.md)

> **本地优先的 AI 工作会话与知识管理桌面应用。** 从多种 AI 编程助手的历史对话与公开分享链接中整理知识，支持可追踪的提炼流水线、知识工作台、全文/语义检索与 SiYuan 同步。

> **授权变更提示：** AIKS 由原 MIT 授权转为 **PolyForm Noncommercial 1.0.0（非商业使用）**，仅适用于授权方拥有相应权利的 AIKS 代码。该许可**不是 OSI 认可的开源许可**。旧版本的既有 MIT 权利不受追溯影响；第三方软件仍遵守各自许可。详见 [LICENSE](LICENSE)、[授权范围](LICENSE-SCOPE.md) 与 [第三方声明](THIRD_PARTY_NOTICES.md)。

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

AIKS 受许多优秀项目启发，部分解析实现也基于第三方 MIT 许可代码进行适配。特别感谢：

- [SiYuan / 思源笔记](https://github.com/siyuan-note/siyuan) — 内嵌知识文档引擎和 HTTP API；SiYuan 为 **AGPL-3.0**，其授权及版权与 AIKS **独立**。
- [AICoder Session Viewer](https://github.com/seastart/aicoder-session-viewer) — Claude/Codex/Gemini/OpenCode 会话解析的早期重要参考与改编来源（MIT）。
- [CC Switch](https://github.com/farion1231/cc-switch)、[ccusage](https://github.com/ccusage/ccusage) — 数据源发现、会话扫描及使用信息处理的参考。
- [OpenAI Codex](https://github.com/openai/codex)、[Gemini CLI](https://github.com/google-gemini/gemini-cli)、[OpenCode](https://github.com/anomalyco/opencode)、[Claude Code](https://github.com/anthropics/claude-code) — Provider 格式及行为兼容参考；引用并不代表项目背书。
- [chatgpt-share-parser](https://github.com/evanhu1/chatgpt-share-parser)、[chat-share-reader](https://github.com/pencil311/chat-share-reader) 等 — 公开分享对话格式研究及解析参考；准确来源、改编范围与许可证以 [第三方声明](THIRD_PARTY_NOTICES.md) 为准。
- [Tauri](https://github.com/tauri-apps/tauri)、[React](https://github.com/facebook/react)、[Radix UI](https://github.com/radix-ui/primitives) 及 Rust 社区 — 桌面应用和 UI 基础设施。

上述项目商标和版权属于各自所有者；AIKS 与这些项目不存在当然的官方隶属或认可关系。**详细许可证、固定版本、参考/改编范围：** [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)。

## 授权、贡献与免责声明

- **AIKS 自有且授权方有权重新许可的代码：** [PolyForm Noncommercial 1.0.0](LICENSE)；非商业研究、个人学习等在许可范围内。商业用途需另行取得相关权利人的书面授权。
- **第三方和旧版：** 不改变 SiYuan（AGPL-3.0）及其他依赖/移植代码原有许可证；此前已经以 MIT 发布的版本，其既有许可证与已授予权利继续有效。范围请见 [LICENSE-SCOPE.md](LICENSE-SCOPE.md)。
- **关于“开源”：** 非商业使用限制不符合 OSI 开源定义，本仓库对受此限制的部分使用 **source-available / 源码可见** 的表述。
- 欢迎讨论、Issue 和合规贡献；提交前阅读 `AGENTS.md`，并确保你有权为贡献内容授予相应许可。
- 软件按现状提供，不承诺一定兼容所有 Provider 版本，也不保证提炼结果完全准确。执行作废、重建或升级前请备份重要资料。

---
如果你使用了 AIKS，欢迎反馈可重现的问题和兼容性信息。感谢每一位上游作者与贡献者。
