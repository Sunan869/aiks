# AIKS Desktop 项目现状文档

**版本：V2.5**  
**日期：2026-09-12**  
**打包结果：`AIKS_0.2.0_x64-setup.exe` (69.8 MB) ✅**

---

## 一、项目定位

AIKS 是企业内部研发人员的 **AI 工作知识库**。  
自动采集 OpenCode / Codex / Gemini CLI / Claude Code 的 AI 编程会话，  
通过OpenAI-compatible vLLM（Qwen3.8-27B）将原始对话提炼为可搜索、可复用的工程知识。

```
使用 AI 工具
     ↓ 自动采集
AIKS Session Sync
     ↓ 写入
10 AI Sessions（原始会话）
     ↓ AI 提炼
Qwen3.8-27B (http://127.0.0.1:11434/v1)
     ↓ 结构化
20 Knowledge（精炼知识）
     ↓
全文搜索 / 双链 / AI 问答（SiYuan）
```

---

## 二、验证结果

| 指标 | 结果 |
|------|------|
| `cargo check --workspace` | ✅ 全部通过 |
| `cargo test -p aiks-core --lib` | ✅ 69 tests, 0 failed |
| `npm run build` | ✅ TypeScript 无错误 |
| `build-release.ps1` | ✅ 一次成功 |
| NSIS Installer | ✅ 69.8 MB |
| MSI Installer | ✅ 90.6 MB |

---

## 三、架构总览

### Cargo Workspace

```
ai-knowledge-sync/
├── Cargo.toml                    (workspace)
├── crates/
│   └── aiks-core/               (library, 所有核心逻辑)
│       ├── src/
│       │   ├── ai/              (AI 提炼模块)
│       │   ├── bootstrap/       (零配置启动)
│       │   ├── config/          (配置)
│       │   ├── engine/          (AiksEngine facade)
│       │   ├── knowledge/       (知识渲染与服务)
│       │   ├── model/           (Canonical Model)
│       │   ├── providers/       (4个Provider)
│       │   ├── renderer/        (Markdown渲染)
│       │   ├── runtime/         (SiYuan进程管理)
│       │   ├── sink/            (SiYuan HTTP Sink)
│       │   ├── storage/         (SQLite State)
│       │   ├── sync/            (Sync Engine)
│       │   ├── util/            (Sanitizer/Archive)
│       │   └── watcher/         (File Watcher)
│       └── migrations/
│           └── 001_init.sql     (含 knowledge_extraction 表)
│
├── apps/
│   ├── aiks-cli/                (CLI 工具)
│   └── aiks-desktop/            (Tauri 2 桌面应用)
│       ├── src/                 (React Control Center)
│       └── src-tauri/           (Rust 后端)
│           └── resources/siyuan/ (SiYuan 3.8.3 内置运行时)
│
├── scripts/
│   ├── setup-siyuan.ps1         (下载/准备 SiYuan Runtime)
│   ├── dev.ps1                  (开发模式启动)
│   └── build-release.ps1        (Release 构建)
│
└── siyuan.version               (版本锁定: 3.8.3, SHA256 已验证)
```

---

## 四、Rust 源代码统计

| 模块 | 行数 | 功能 |
|------|------|------|
| `ai/client.rs` | ~150 | OpenAI-compatible HTTP Client |
| `ai/extractor.rs` | ~130 | Session → Knowledge 提炼 |
| `ai/chunker.rs` | ~160 | 长 Session 分块（Map-Reduce）|
| `ai/schema.rs` | ~80 | KnowledgeDocument JSON Schema |
| `ai/prompts.rs` | ~120 | Prompt 模板（v1） |
| `ai/config.rs` | ~50 | AiModelConfig |
| `knowledge/renderer.rs` | ~180 | JSON → SiYuan Markdown |
| `knowledge/service.rs` | ~200 | 提炼队列 + 状态管理 |
| `knowledge/model.rs` | ~80 | ExtractionRecord/Status |
| `sync/engine.rs` | ~300 | **V2.5 重写**：修复 minimum_messages 过滤 |
| `runtime/mod.rs` | ~400 | SiYuan 进程管理（serve 命令，无 Token）|
| `bootstrap/mod.rs` | ~150 | 零配置启动 |
| `engine/mod.rs` | ~450 | AiksEngine + FullStatus + sync_with_extraction |
| `providers/*` | ~2500 | 4个Provider |
| `storage/*` | ~700 | SQLite State DB |
| `sink/siyuan.rs` | ~400 | SiYuan HTTP API（Embedded/External 双模式）|
| **总计** | **~14,000** | Rust 源码行数 |

---

## 五、V2.5 核心修复清单

### P0-1: Raw Session Sync 修复

**问题根因**：`minimum_messages` 默认值为 2，而 Codex `state_5.sqlite` 快速路径为所有 session 设置 `message_count=0`，导致 134 条 Codex session 全部被过滤跳过。

**修复**：
- `ContentConfig.minimum_messages` 默认值从 `2` → `0`（同步所有 session）
- `ContentConfig.minimum_session_chars` 默认值从 `100` → `0`
- 过滤逻辑改为：只有 `message_count > 0 AND message_count < minimum` 时才跳过
- 加载 session 后再次检查实际消息数（处理 Codex count=0 的情况）

**修复文件**：`crates/aiks-core/src/config/mod.rs`, `crates/aiks-core/src/sync/engine.rs`

### P0-2: SyncStats 增加 extraction_candidates

**修复**：`SyncStats` 新增 `extraction_candidates: Vec<String>` 字段，Sync 成功后返回 session ID 列表供 AI 提炼队列使用。

### P0-3: 统一 UI 状态模型

**问题**：UI 各页面数字来自不同数据源（Provider scan vs SQLite），出现 "660 vs 28" 不一致。

**修复**：新增 `AiksEngine::full_status()` 方法，返回 `FullStatus` struct，包含：
- `scan_total` / `scan_by_source`（Provider 扫描结果）
- `db_total` / `db_synced` / `db_pending` / `db_failed`（SQLite 状态）
- `last_sync_*`（最近同步统计）
- `extraction_*`（AI 提炼统计）

对应 Tauri Command：`get_full_status()`

### P0-4: 修正按钮语义

| 之前 | 之后 |
|------|------|
| "立即整理" | "立即同步" |
| 数据源页触发 AI 提炼 | 数据源页触发 Raw Sync |
| sync_now | sync_and_extract（同步后自动入队提炼）|

### P0-5: 同步记录页真实统计

`SyncPage.tsx` 现在从 `FullStatus` 读取真实数据：
- 发现 / 新增 / 更新 / 失败
- AI 提炼：待整理 / 成功 / 跳过 / 失败

### P0-6: NSIS 升级安装生命周期

添加 `src-tauri/nsis/installer-hooks.nsh`，在安装前：
1. 发送 `WM_CLOSE` 给 AIKS 主窗口
2. 等待 8 秒让进程优雅退出
3. 超时后强制 `taskkill /F`
4. 关闭 SiYuan-Kernel.exe（AIKS 自己启动的）
5. 保留用户数据（aiks.db / workspace / settings）

### 启动流程改进

`lifecycle.rs` 改为三阶段清晰日志：

```text
[STARTUP] Beginning initial scan...
[SCAN] Discovered 660 sessions (Codex=134, OpenCode=518, Gemini=8, Claude=0)
[SYNC] Starting initial raw sync for 660 sessions
[SYNC] Complete: discovered=660 new=660 updated=0 unchanged=0 skipped=0 failed=0
[EXTRACT] Queuing N sessions for AI extraction
```

---

## 六、新增 Tauri Commands（V2.5）

| Command | 功能 |
|---------|------|
| `get_full_status` | 统一状态（解决 660/28 不一致）|
| `sync_and_extract` | Raw Sync + 自动入队 AI 提炼 |
| `get_ai_status` | AI 模型健康 + 提炼统计 |
| `test_ai_connection` | 测试 vLLM 连接 |
| `extract_session_now` | 手动立即提炼指定 Session |
| `get_knowledge_stats` | 知识提炼统计 |
| `get_recent_knowledge` | 最近精炼知识列表 |
| `open_knowledge_window` | 打开 SiYuan 知识库窗口 |

---

## 七、React UI 页面清单

| 页面 | 文件 | 状态 |
|------|------|------|
| 概览 | `OverviewPage.tsx` | 使用 `FullStatus`，显示 4 格指标卡 + 同步按钮 |
| 知识库 | `KnowledgePage.tsx` | 显示提炼知识列表 + 打开 SiYuan 按钮 |
| 数据源 | `SourcesPage.tsx` | 显示已连 Provider + **"立即同步"**（非"立即整理"）|
| 同步记录 | `SyncPage.tsx` | 显示真实 Raw Sync + AI 提炼统计 |
| 设置 | `SettingsPage.tsx` | 包含 AI 智能整理设置区 |
| 帮助与诊断 | `DiagnosticsPage.tsx` | 系统诊断 + 重启知识引擎 |

---

## 八、AI Knowledge Extractor

### 模型配置

```
Base URL: http://127.0.0.1:11434/v1
Model:    Qwen3.8-27B
API:      POST /v1/chat/completions (OpenAI-compatible)
Temperature: 0.1
Max Tokens:  4096
Timeout:     120s
Min Score:   0.6（低于此分数 → 仅保留 Raw Session）
Chunk Size:  40 messages / chunk
Concurrent:  1
```

### 提炼流程

```
NormalizedSession
     ↓ Secret Sanitizer（脱敏）
     ↓ Chunker（> 40 msgs → Map-Reduce）
     ↓ 每个 Chunk → Chunk Summary JSON
     ↓ Final Extraction Prompt
     ↓ Qwen3.8-27B
     ↓ KnowledgeDocument JSON（结构化输出）
     ↓ Schema 验证（失败 → 一次重试修复）
     ↓ Markdown Renderer（按 category 生成不同结构）
     ↓ SiYuan / 20 Knowledge
```

### 知识类别

| 类别 | 文档结构 |
|------|----------|
| `troubleshooting` | 问题 → 现象 → 根因 → 解决方案 → 关键命令 |
| `implementation` | 目标 → 方案 → 关键模块 → 验证结果 → 后续 |
| `design` | 背景 → 设计决策 → 方案要点 |
| `research` | 研究问题 → 结论 |
| `decision` | 背景 → 决策内容 → 理由 |
| `general` | 摘要 → 解决方案 → 决策 |

### AI 失败降级

模型不可用时：
- Raw Session 仍正常写入 `10 AI Sessions`
- Extraction 标记为 `FAILED`（可重试）
- UI 显示"AI 暂时不可用"，不阻塞 Raw Sync

---

## 九、数据库结构

**路径**：`%LOCALAPPDATA%\AIKnowledgeSync\aiks.db`

| 表 | 用途 |
|----|------|
| `source_session` | 所有发现的 Session（hash / status / 路径）|
| `sync_target` | Session → SiYuan 同步状态 |
| `source_file_state` | JSONL/JSON 文件增量状态 |
| `sync_run` | 每次同步的统计记录 |
| `knowledge_extraction` | AI 提炼状态（score / category / doc_id）|

---

## 十、SiYuan 集成

### 启动方式（SiYuan 3.8.3 serve 命令）

```
SiYuan-Kernel.exe serve
  --workspace=%LOCALAPPDATA%\AIKnowledgeSync\siyuan\workspace
  --wd=<resources/siyuan>
  --port=<auto-allocated 6806-6899>
  --lang=zh-CN
  --mode=prod
```

**不传 `--accessAuthCode`**（Embedded 模式，loopback-only，本机无需鉴权）

### 知识库结构

```
AI Knowledge
├── 10 AI Sessions
│   ├── OpenCode
│   │   └── {year}/{month}/{date} {title} [{session_id}]
│   ├── Codex
│   ├── Gemini
│   └── Claude
│
├── 20 Knowledge
│   └── {project}/{category}/{date} {title} [{session_id}]
│
└── 90 System
```

### AIKS 托管属性

每个文档写入：
```
custom-aiks-managed=true
custom-aiks-source=opencode
custom-aiks-session-id=ses_f717
custom-aiks-content-hash=sha256...
custom-aiks-parser-version=opencode-sqlite-v1
custom-aiks-synced-at=2026-09-12T...
```

---

## 十一、Provider 状态

| Provider | 数据路径 | 格式 | Parser 版本 |
|----------|----------|------|------------|
| OpenCode | `~/.local/share/opencode/opencode.db` | SQLite（session/message/part 表）| `opencode-sqlite-v1` |
| Codex | `~/.codex/sessions/Y/M/D/rollout-*.jsonl` | JSONL（双格式兼容）| `codex-v1` |
| Gemini CLI | `~/.gemini/tmp/{project}/chats/session-*.json` | JSON + .project_root | `gemini-json-v1` |
| Claude Code | `~/.claude/projects/{hash}/{uuid}.jsonl` | JSONL（双 Pass）| `claude-v1` |

---

## 十二、开发与发布流程

```powershell
# 第一次克隆后（准备 SiYuan 运行时）
powershell -ExecutionPolicy Bypass -File scripts\setup-siyuan.ps1

# 开发模式
powershell -ExecutionPolicy Bypass -File scripts\dev.ps1

# Release 构建
powershell -ExecutionPolicy Bypass -File scripts\build-release.ps1
# 输出: target/release/bundle/nsis/AIKS_0.2.0_x64-setup.exe (69.8 MB)

# CLI 工具
.\target\debug\aiks.exe doctor
.\target\debug\aiks.exe scan
.\target\debug\aiks.exe sync --dry-run
.\target\debug\aiks.exe daemon
```

---

## 十三、V2.5 相比 V2.0 关键差异

| 方面 | V2.0 | V2.5 |
|------|------|------|
| Codex session 同步 | 全部跳过（count=0 < min=2）| 全部参与同步（min=0）|
| UI 状态一致性 | 多个数据源（660/28 并存）| 统一 `FullStatus` 单数据源 |
| 按钮语义 | "立即整理"（错误）| "立即同步"（正确）|
| 同步记录 | 全 0 | 真实统计（发现/新增/更新/失败）|
| AI 提炼入口 | 与同步分离，孤立 | `sync_and_extract`：同步后自动入队 |
| 启动日志 | 简单 | 三阶段：SCAN → SYNC → EXTRACT 带完整计数 |
| 升级安装 | SiYuan-Kernel.exe 阻塞安装 | NSIS hook 优雅关闭旧实例 |
| 单窗口 | 两个窗口同时弹出 | 仅 Control Center，知识库按需打开 |

---

## 十四、当前已知限制

| 限制 | 说明 |
|------|------|
| AI 提炼队列 | 基础队列已实现，Worker 循环待实际运行验证 |
| 实时文件监听 | notify crate 架构已集成，事件触发同步待 E2E 测试 |
| 知识库内嵌 | 知识库在独立窗口打开（按需），未内嵌主窗口 |
| Windows 升级 | NSIS hook 已添加，完整升级场景待验证 |
| 项目视图 | 已有项目名识别，前端 UI 未单独实现项目视图 |

---

## 十五、下一步 P1 优先级

1. **E2E 验证**：启动 AIKS，确认 660 条 session 写入 SiYuan `10 AI Sessions`
2. **vLLM 验证**：确认 `ses_f717` 经过 Qwen3.8-27B 提炼，在 `20 Knowledge` 中生成知识文档
3. **Knowledge UI 真实数据**：知识页从 `20 Knowledge` 读取并展示
4. **项目视图**：按 `project_name / project_path` 分组知识
5. **全局搜索**：顶部搜索栏（依托 SiYuan 全文搜索）
6. **收藏**：利用 SiYuan 属性/书签实现
7. **历史重新整理**：清除 extraction hash 触发重提炼
