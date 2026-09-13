# AIKS V2.5 核心链路修复与企业内用增量迭代实施文档

版本：V1.1  
日期：2026-09-12  
适用项目：AI Knowledge Sync / AIKS Desktop  
目标：修复当前“可扫描但未真正沉淀知识”的核心链路问题，并补齐公司内部长期使用所需的升级安装、诊断和单窗口体验。

## 1. 本轮结论

当前版本不是 Provider 发现失败，而是 **UI 已完成、核心 E2E 数据链路未闭环**。已经能发现 OpenCode 518、Codex 134、Gemini CLI 8，共 660 条 Session，但同步记录仍为 0、Embedded SiYuan 中只有空的 `AI Knowledge`、知识提炼队列也为 0。

因此本轮禁止继续优先做纯视觉优化，优先完成：

`Provider Scan → Raw Session Sync → SiYuan → Extraction Queue → Qwen3.8-27B → KnowledgeDocument → Knowledge UI`

同时把上一轮遗留的 **升级安装生命周期** 纳入 P0：安装新版前自动优雅关闭旧版 AIKS 与其 Embedded `SiYuan-Kernel.exe`，避免出现 “Error opening file for writing”。

## 2. 当前版本现象与判断

| 现象 | 当前状态 | 判断 |
|---|---:|---|
| OpenCode | 518 | Provider Scan 正常 |
| Codex | 134 | Provider Scan 正常 |
| Gemini CLI | 8 | Provider Scan 正常 |
| 总 Session | 660 | UI 已拿到扫描结果 |
| 同步记录 | 发现0/变更0/同步0/失败0 | 真正 Sync 未执行或未记录 |
| SiYuan | 仅空 `AI Knowledge` | Raw Session 未写入 |
| 精炼知识 | 0 | Extraction Queue 未入队 |
| 待整理 | 0 | 同上 |
| UI 左下“历史对话” | 28 | 与 660 不一致，存在状态源分叉 |
| “打开知识库” | 打开独立 AIKS Knowledge | 单窗口目标只完成一半 |

## 3. 本轮 P0 目标

1. **打通 Raw Session 同步**：660 条扫描结果必须真正进入 Sync Engine，并写入 Embedded SiYuan。
2. **打通 AI 知识提炼**：同步成功后按策略进入 Extraction Queue，调用公司 vLLM `Qwen3.8-27B`。
3. **统一 UI 数据源**：概览、侧边栏、数据源、同步记录、知识库的数字必须来自统一状态模型。
4. **修正业务按钮语义**：数据源页优先是“立即同步”，不是“立即整理”。
5. **单窗口体验收口**：默认只显示一个 AIKS 主窗口；知识内容在主窗口内查看，独立 Knowledge Window 仅作为临时降级方案。
6. **模型不可用不影响 Raw Sync**：AI 整理必须是派生流程。
7. **升级安装生命周期**：安装新版时自动处理旧版 AIKS/Kernel 进程并保留用户数据。

## 4. 正确目标架构

```text
OpenCode / Codex / Gemini / Claude
                ↓
          Provider Scan
                ↓
       NormalizedSession
                ↓
          Raw Sync Engine
                ↓
      Embedded SiYuan Sink
                ↓
      10 AI Sessions / ...
                ↓
     Knowledge Extraction Queue
                ↓
        Qwen3.8-27B (vLLM)
                ↓
        KnowledgeDocument JSON
                ↓
          Markdown Renderer
                ↓
          20 Knowledge / ...
                ↓
          AIKS Knowledge UI
```

其中：

- Raw Session 是 Source of Truth。
- AI Knowledge 是派生结果。
- AI 失败不能回滚 Raw Session 同步。
- Session 发生变化时，只根据 `source_content_hash` 做增量重提炼。

## 5. Raw Session Sync 修复要求

### 5.1 Desktop 启动必须按三个阶段执行

```text
scan      = 发现源数据
sync      = 写入 Raw Session 到知识库
extract   = AI 提炼为精炼知识
```

三个阶段必须有独立状态，不得再用一个“同步中”同时代表全部流程。

### 5.2 首次启动流程

```text
1. 启动 Embedded SiYuan
2. Provider Scan
3. 得到 660 Session
4. UI 立即可用
5. 后台 Initial Raw Sync
6. Raw Session 写入 10 AI Sessions
7. 每个成功同步 Session 按策略 enqueue extraction
8. AI Worker 后台整理
```

### 5.3 SiYuan 目录结构

首次同步完成后至少应存在：

```text
AI Knowledge
├── 10 AI Sessions
│   ├── OpenCode
│   ├── Codex
│   ├── Gemini
│   └── Claude
├── 20 Knowledge
└── 90 System
```

即使 AI 模型完全不可用，`10 AI Sessions` 也必须存在并包含 Raw Session 文档。

### 5.4 数据源页按钮

当前“立即整理”修改为：

- `立即同步`：执行 Raw Session Scan + Sync。
- Session 已同步后才允许 `智能整理`。
- Knowledge 页可提供 `整理全部候选`。

## 6. AI Knowledge Extractor 修复要求

### 6.1 模型配置

公司内部 vLLM：

```text
Base URL: http://10.10.23.16:18000/v1
Model:    Qwen3.8-27B
```

通过 OpenAI-compatible API 调用，第一版使用：

```text
POST /v1/chat/completions
```

### 6.2 AI 提炼必须排队执行

```text
Raw Sync Success
      ↓
enqueue extraction
      ↓
background worker
      ↓
Qwen3.8-27B
```

禁止在同步线程内等待模型。

第一版建议并发：1～2。

### 6.3 Session 触发策略

- 自动整理新会话：默认 ON。
- Session 10 分钟无变化后自动整理。
- 用户可以手动“立即整理”。
- `source_content_hash` 未变化不得重复调用模型。

### 6.4 长 Session

长会话采用 Map/Reduce：

```text
Session
 ↓
20k～30k tokens / chunk
 ↓
Chunk Summaries
 ↓
Final Knowledge Extraction
```

必须保留：关键错误、命令、接口、文件路径、类/函数名、版本、配置、最终设计决策。

### 6.5 Structured JSON

模型不得直接自由生成最终 Markdown。应先输出结构化 JSON，至少包含：

- `worth_extracting`
- `knowledge_score`
- `title`
- `summary`
- `project`
- `category`
- `tags`
- `problem`
- `root_causes`
- `solutions`
- `decisions`
- `key_commands`
- `key_files`
- `todos`
- `confidence`

AIKS 校验 JSON 后再统一渲染 Markdown。

## 7. Knowledge 页面与单窗口体验

### 7.1 启动行为

启动 AIKS 时只显示一个主窗口。

禁止启动时自动弹出 `AIKS Knowledge`。

### 7.2 Knowledge 主页面

左侧“知识库”进入 AIKS 自己的知识视图，而不是直接把完整 SiYuan 首页作为默认 UI。

推荐视图：

```text
全部 | 最近 | 项目 | 类型 | 来源 | 标签 | 收藏
```

知识列表默认展示精炼知识；原始 Session 可通过“查看原始会话”进入。

### 7.3 SiYuan 的定位

SiYuan 继续负责存储、编辑、搜索和知识组织，但普通内部用户不应感知 `Kernel / Port / Token / SiYuan` 等技术概念。

如果短期内嵌 WebView 技术风险较高，可以暂时按需打开 `AIKS - 知识库`，但：

- 启动时不创建；
- 第一次点击才创建；
- 后续复用；
- 关闭不退出 AIKS；
- 这是过渡方案，不是最终完成标准。

## 8. UI 状态模型统一

当前出现“660”和“28”并存，说明 UI 使用了多个状态源。

必须建立统一 `AppStatus / SyncStatus / KnowledgeStatus`：

```text
Provider Scan Count
Raw Synced Count
Pending Raw Sync
Extraction Pending
Extraction Running
Knowledge Success
Knowledge Skipped
Knowledge Failed
Last Sync Time
Last Extraction Time
```

概览、侧边栏、数据源、同步记录、知识库都从同一后端状态接口读取。

禁止：

- 页面各自重新 scan；
- 一部分读 Provider，一部分读 SQLite 聚合；
- 显示层自己猜状态。

## 9. 同步记录页面

首次 Raw Sync 不能显示全 0。

合理示例：

```text
发现       660
新增       660
更新         0
同步       660
失败         0
```

之后增量同步：

```text
发现       660
新增         2
更新         3
无变化     655
失败         0
```

AI 整理单独显示：

```text
待整理       8
整理成功     5
已跳过       2
失败         1
```

## 10. 日志与诊断

必须补充真实链路日志，示例：

```text
[SCAN] OpenCode discovered=518
[SCAN] Codex discovered=134
[SCAN] Gemini discovered=8

[SYNC] discovered=660 new=660 updated=0 unchanged=0
[SYNC] synced=660 failed=0

[EXTRACT] queued=210
[EXTRACT] session=ses_f717 model=Qwen3.8-27B status=RUNNING
[EXTRACT] session=ses_f717 score=0.91 status=SUCCESS
```

帮助与诊断页增加：

- Provider 状态
- Embedded SiYuan 状态
- Raw Sync 状态
- Extraction Queue 状态
- vLLM 健康状态
- 导出诊断包

诊断包不得包含完整聊天正文、Token、密码、API Key、个人知识正文。

## 11. 上一轮新增 P0：升级安装生命周期

### 11.1 当前问题

新版安装覆盖旧版时，如果 AIKS 已关闭到托盘或 `SiYuan-Kernel.exe` 仍在运行，NSIS 会出现：

```text
Error opening file for writing:
...\AIKSesources\siyuan\kernel\SiYuan-Kernel.exe
```

### 11.2 目标行为

用户双击新版安装包：

```text
检测旧 AIKS 正在运行
      ↓
提示需要关闭
      ↓
请求 AIKS 正常退出
      ↓
停止 Watcher / Sync / Extraction Queue
      ↓
调用 /api/system/exit
      ↓
等待 Kernel 最多 8 秒
      ↓
必要时安全强杀属于 AIKS 的 Kernel
      ↓
安装继续
```

### 11.3 安装器要求

1. 安装前检测旧 AIKS 实例。
2. 优先优雅退出，不第一时间 `taskkill /F`。
3. 必须等待 `AIKS.exe` 与其 Embedded `SiYuan-Kernel.exe` 退出后再覆盖文件。
4. 禁止通过“忽略”继续形成新旧 Runtime 混装。
5. 无响应时允许用户确认“强制关闭后继续”。
6. 只能杀 AIKS 自己安装目录 / workspace 对应的 Kernel，不得误杀用户独立安装的 SiYuan。
7. 升级必须保留：
   - `aiks.db`
   - archive
   - SiYuan workspace
   - 用户设置
   - enterprise policy
8. 安装完成可勾选“启动 AIKS”。

### 11.4 升级验收

必须真实验证：

- 主窗口打开时升级；
- 关闭到托盘时升级；
- Knowledge 正在打开时升级；
- Extraction Queue 正在运行时升级；
- Kernel 运行时升级；
- AIKS 完全关闭时升级；
- 升级后原有 660 Session 状态和知识库仍在。

## 12. E2E 强制验收用例

使用真实 OpenCode Session：

```text
ses_f717
```

完整验证：

```text
OpenCode DB
 ↓
读取 ses_f717（约 140 messages）
 ↓
NormalizedSession
 ↓
Raw Sync
 ↓
SiYuan / 10 AI Sessions / OpenCode
 ↓
Extraction Queue
 ↓
Qwen3.8-27B
 ↓
合法 Knowledge JSON
 ↓
Markdown Renderer
 ↓
SiYuan / 20 Knowledge
 ↓
AIKS Knowledge 页面可见
 ↓
可跳回 Raw Session
```

同时验证：

- Qwen 停机时 Raw Sync 仍成功；
- Qwen 恢复后 Queue 可续跑；
- Session 内容不变不重复提炼；
- Session 更新后原 Knowledge 更新而不是重复创建。

## 13. 实施顺序

### P0-1：Raw Sync

先让 660 条 Session 真正写入 SiYuan。

### P0-2：状态统一

消除 660 / 28 等不一致。

### P0-3：Extraction Queue

同步成功后正确入队。

### P0-4：Qwen E2E

用 `ses_f717` 完成真实调用和知识文档生成。

### P0-5：Knowledge UI

知识列表从真实 `20 Knowledge` 展示。

### P0-6：单窗口收口

知识查看回归主窗口体验。

### P0-7：升级安装生命周期

解决运行中升级无法覆盖 Kernel 的问题。

之后再做 P1：项目视图、全局搜索、收藏、最近查看、内部自动更新等体验增强。

## 14. 禁止事项

本轮禁止：

- 继续优先做纯 UI 美化；
- 重写 Provider；
- 重写 Canonical Model；
- 新建向量数据库；
- 新建全文搜索引擎；
- 新建编辑器；
- 新建独立 RAG；
- 用单元测试通过替代真实 Desktop E2E。

## 15. Coding Agent 完成标准

只有以下全部提供证据，才能标记完成：

1. `cargo check --workspace` 通过。
2. `cargo test --workspace` 通过。
3. `npm run build` 通过。
4. Tauri dev 可运行。
5. Release Installer 可构建。
6. SiYuan 中真实存在 Raw Session 文档。
7. `ses_f717` Raw Session 可查看。
8. vLLM 实际收到请求。
9. Qwen3.8-27B 返回合法 Knowledge JSON。
10. `20 Knowledge` 中生成真实知识文档。
11. Knowledge 页面显示真实知识，而不是 0。
12. UI 各处 Session 数一致。
13. Qwen 不可用时 Raw Sync 不受影响。
14. 新版安装可自动处理旧 AIKS/Kernel 进程并保留用户数据。

## 16. 最终目标

本轮完成后，AIKS 应真正达到：

```text
使用 OpenCode / Codex / Gemini
          ↓
AIKS 自动发现
          ↓
Raw Session 自动同步
          ↓
公司内部 Qwen3.8-27B 自动整理
          ↓
形成简洁、可复用的工程知识
          ↓
员工可以持续搜索、查看和复用
```

最终定位：**AI Work Knowledge System，而不是 AI Session Viewer。**
