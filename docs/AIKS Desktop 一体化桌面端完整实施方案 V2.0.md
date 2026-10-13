# AIKS Desktop 一体化桌面端完整实施方案 V2.0

版本：V2.0  
目标平台：Windows 11 优先  
现有基础：AIKS V1 Rust Core 已完成  
桌面框架：Tauri 2 + React + TypeScript  
知识库引擎：内置 SiYuan Kernel  
目标体验：安装即用、启动即用、零外部依赖、零手工配置

---

# 1. V2 最终目标

V1 当前产品形态为：

```text
用户
 │
 ├─ 手动启动 SiYuan
 │
 ├─ 配置 SIYUAN_TOKEN
 │
 ├─ 启动 aiks daemon
 │
 └─ 打开 SiYuan 查看知识
```

V2 必须改为：

```text
用户
 │
 │ 双击
 ▼
AIKS Desktop.exe
 │
 ├─ 自动启动内置 SiYuan
 ├─ 自动初始化 Workspace
 ├─ 自动初始化鉴权
 ├─ 自动发现 AI 应用
 ├─ 自动启动 Session Collector
 ├─ 自动启动文件监听
 ├─ 自动同步
 │
 ▼
AIKS Desktop UI
 │
 └─ 直接查看、搜索、编辑知识
```

用户最终只需要看到：

```text
AIKS
```

不能要求用户了解：

```text
SiYuan Kernel
6806
SIYUAN_TOKEN
config.toml
aiks daemon
cargo
PowerShell
```

这些全部属于内部实现。

---

# 2. 产品形态

最终交付：

```text
AIKS-Setup-x64.exe
```

用户：

```text
安装
  ↓
桌面出现 AIKS
  ↓
双击
  ↓
直接使用
```

禁止要求：

```text
安装 Rust
安装 Node.js
安装 Go
安装 SiYuan
安装 Docker
配置环境变量
打开命令行
```

---

# 3. 一个程序不等于一个进程

这里需要明确。

最终用户只启动：

```text
AIKS Desktop.exe
```

但是内部允许：

```text
AIKS Desktop.exe
        │
        ├── AIKS Rust Core
        │
        ├── Session Collector
        │
        ├── Sync Engine
        │
        └── SiYuan-Kernel.exe
```

其中：

```text
SiYuan-Kernel.exe
```

由 AIKS 自动作为 child process / sidecar 启动。

用户：

```text
不安装
不配置
不启动
不关闭
```

SiYuan。

Tauri 2 官方支持将外部二进制作为 sidecar 打包并由 Rust 启动，因此 SiYuan Kernel 应按这一机制管理。

---

# 4. 最终总体架构

```text
┌─────────────────────────────────────────────────┐
│                AIKS Desktop                     │
│              Tauri 2 Desktop App               │
│                                                 │
│ ┌─────────────────────────────────────────────┐ │
│ │              Desktop UI                    │ │
│ │                                             │ │
│ │ Knowledge / Search / Edit                  │ │
│ │ AI Sources                                 │ │
│ │ Sync Status                                │ │
│ │ Settings                                   │ │
│ │ Logs                                       │ │
│ └───────────────────┬─────────────────────────┘ │
│                     │                           │
│                     ▼                           │
│ ┌─────────────────────────────────────────────┐ │
│ │               AIKS Core                    │ │
│ │                                             │ │
│ │ Provider Registry                          │ │
│ │ Canonical Model                            │ │
│ │ Sync Engine                                │ │
│ │ File Watcher                               │ │
│ │ State SQLite                               │ │
│ │ Markdown Renderer                          │ │
│ │ Archive                                    │ │
│ └───────────────┬─────────────────────────────┘ │
│                 │                               │
│                 │ Public HTTP API               │
│                 ▼                               │
│ ┌─────────────────────────────────────────────┐ │
│ │        Embedded SiYuan Runtime             │ │
│ │                                             │ │
│ │ SiYuan-Kernel.exe                          │ │
│ │ stage                                      │ │
│ │ appearance                                 │ │
│ │ guide                                      │ │
│ └───────────────────┬─────────────────────────┘ │
│                     │                           │
└─────────────────────┼───────────────────────────┘
                      │
             Local Session Sources
                      │
      ┌───────────────┼────────────────┐
      ▼               ▼                ▼
    Codex          OpenCode          Gemini
                                       │
                                    Claude
```

SiYuan 官方桌面资源本身即包含 `appearance / guide / stage / kernel`，Kernel 可以独立以 `serve` 模式运行。自 SiYuan 3.7.0 起启动 HTTP 服务必须显式使用 `serve` 子命令，并支持 workspace、port、accessAuthCode 等参数。

---

# 5. 对当前 6518 行代码的处理

禁止推翻 V1 重写。

当前代码：

```text
providers/
renderer/
sink/
storage/
sync/
util/
config/
model/
```

全部属于已经验证的 Core。

需要执行：

```text
当前 CLI 项目
      ↓
抽离
      ↓
aiks-core library
      ↓
同时供
 ┌────┴────┐
 │         │
CLI      Desktop
```

重构后：

```text
workspace/
│
├── crates/
│   └── aiks-core/
│
├── apps/
│   ├── aiks-cli/
│   └── aiks-desktop/
│
├── migrations/
├── fixtures/
├── docs/
└── references/
```

---

# 6. 新项目目录

推荐最终结构：

```text
ai-knowledge-sync/
│
├── Cargo.toml
│
├── crates/
│   └── aiks-core/
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs
│           ├── config/
│           ├── model/
│           ├── providers/
│           ├── renderer/
│           ├── sink/
│           ├── storage/
│           ├── sync/
│           ├── watcher/
│           ├── runtime/
│           └── util/
│
├── apps/
│
│   ├── aiks-cli/
│   │   ├── Cargo.toml
│   │   └── src/main.rs
│   │
│   └── aiks-desktop/
│       │
│       ├── package.json
│       ├── vite.config.ts
│       ├── src/
│       │   ├── App.tsx
│       │   ├── pages/
│       │   ├── components/
│       │   ├── hooks/
│       │   └── services/
│       │
│       └── src-tauri/
│           ├── Cargo.toml
│           ├── tauri.conf.json
│           ├── capabilities/
│           ├── binaries/
│           │   └── siyuan/
│           │       └── SiYuan-Kernel.exe
│           │
│           ├── resources/
│           │   └── siyuan/
│           │       ├── stage/
│           │       ├── appearance/
│           │       └── guide/
│           │
│           └── src/
│               ├── main.rs
│               ├── app_state.rs
│               ├── commands.rs
│               ├── tray.rs
│               ├── siyuan_runtime.rs
│               ├── bootstrap.rs
│               └── lifecycle.rs
│
├── migrations/
├── fixtures/
├── docs/
└── THIRD_PARTY_NOTICES.md
```

---

# 7. AIKS Core 改造

当前：

```text
main.rs
```

承担 CLI 初始化。

必须把业务能力移出 main。

新增：

```rust
pub struct AiksEngine {
    config: AppConfig,
    providers: ProviderRegistry,
    state_db: StateDb,
    sync_engine: SyncEngine,
}
```

提供：

```rust
impl AiksEngine {

    pub async fn initialize(...) -> Result<Self>;

    pub async fn doctor(&self) -> DoctorResult;

    pub async fn scan(&self) -> ScanResult;

    pub async fn sync(
        &self,
        request: SyncRequest
    ) -> Result<SyncResult>;

    pub async fn status(&self) -> AppStatus;

    pub async fn start_watcher(&self);

    pub async fn shutdown(&self);
}
```

CLI：

```text
只是 AiksEngine 的一个调用者
```

Desktop：

```text
也是 AiksEngine 的调用者
```

禁止复制两份业务逻辑。

---

# 8. SiYuan Runtime 内置方案

安装目录内部携带：

```text
AIKS/
│
├── AIKS.exe
│
└── resources/
    └── siyuan/
        ├── kernel/
        │   └── SiYuan-Kernel.exe
        ├── stage/
        ├── appearance/
        └── guide/
```

不要安装：

```text
完整 SiYuan Electron Desktop
```

AIKS 自己已经是桌面 Shell。

只需要 SiYuan：

```text
Kernel
+
Web Stage
+
必要资源
```

官方文档也说明这些正是 SiYuan Electron 安装包 resources 中的核心组成。

---

# 9. SiYuan 版本必须固定

禁止：

```text
构建时 wget latest
```

必须固定：

```text
SIYUAN_VERSION
SIYUAN_COMMIT
SHA256
```

例如：

```text
siyuan.version
```

内容：

```text
version=...
commit=...
sha256=...
```

构建时验证：

```text
binary hash
resource hash
```

近期 SiYuan 出现过多项本地服务安全修复，因此发布版必须使用当前已修复的稳定版本，并在升级时重新执行安全兼容测试；例如部分 3.7.x 问题在 3.7.4 修复，另有 3.8.0 问题在 3.8.1 修复。

不能把未经验证的新 SiYuan 版本直接塞入安装包。

---

# 10. 用户数据目录

程序安装目录：

```text
C:\Program Files\AIKS\
```

只放程序。

所有用户数据：

```text
%LOCALAPPDATA%\AIKnowledgeSync\
```

建议：

```text
AIKnowledgeSync/
│
├── data/
│   ├── aiks.db
│   └── archive/
│
├── siyuan/
│   └── workspace/
│
├── config/
│   └── app.json
│
├── cache/
│
└── logs/
```

不得：

```text
把知识数据存进 Program Files
```

否则升级和卸载风险很大。

---

# 11. 第一次启动流程

这是 V2 最关键的流程。

用户第一次双击：

```text
AIKS.exe
```

程序自动执行：

```text
1. 检查运行环境

2. 创建
   %LOCALAPPDATA%\AIKnowledgeSync

3. 创建 State DB

4. 创建 SiYuan Workspace

5. 找可用本地端口

6. 启动 SiYuan Kernel

7. 等待 SiYuan Ready

8. 初始化 SiYuan API 访问

9. 创建 AI Knowledge Notebook

10. 自动扫描：
    Claude
    Codex
    Gemini
    OpenCode

11. 显示发现结果

12. 后台执行第一次同步

13. 打开知识库主界面
```

整个过程：

```text
不能弹 PowerShell
不能要求配置 Token
不能要求选择端口
```

---

# 12. 启动 SiYuan

当前 SiYuan 自 3.7.0 起应按：

```text
SiYuan-Kernel.exe serve ...
```

启动，而不是旧版本的裸参数启动方式。

逻辑示意：

```text
SiYuan-Kernel.exe
    serve
    --port <port>
    --workspace <workspace>
    --accessAuthCode <generated-secret>
```

端口不要写死：

```text
6806
```

应先：

```text
try 6806
↓
busy
↓
6807
↓
...
```

或者由 PortAllocator 分配。

保存：

```text
runtime.siyuan_port
```

到运行时状态。

---

# 13. 鉴权必须自动化

用户不应该设置：

```text
SIYUAN_TOKEN
```

V2 中这个环境变量只保留为：

```text
Developer Override
```

正式用户完全看不到。

AIKS 第一次创建 Workspace 时：

```text
生成随机 access secret
```

至少：

```text
32 bytes crypto random
```

API Token 同样不得使用：

```text
123456
aiks
password
```

SiYuan API 的官方鉴权方式是：

```text
Authorization: Token <token>
```

公开 API 默认端点为本地 Kernel HTTP 服务。

---

# 14. API Token Bootstrap

推荐实现：

```text
SiYuan Kernel
      ↓
首次生成 Workspace 配置
      ↓
AIKS Bootstrapper
      ↓
只读获取当前 API Token
      ↓
放入进程内 SecretStore
```

注意：

读取 SiYuan：

```text
workspace/conf/conf.json
```

仅允许发生在：

```text
SiyuanBootstrapper
```

模块。

禁止其他模块直接依赖 SiYuan 内部配置文件。

其目的仅是：

```text
bootstrap API authentication
```

正式知识读写仍全部走 SiYuan 公共 API。

如果未来 SiYuan 改变配置结构：

```text
Bootstrap Adapter
```

单独升级。

不能让业务代码依赖 `conf.json`。

---

# 15. Token 存储

Token：

```text
不能写日志
不能展示 UI
不能进入 crash report
```

优先：

```text
Windows Credential Manager
```

保存。

如果 Workspace 重建：

```text
自动重新发现 Token
```

用户无需处理。

---

# 16. SiYuan HTTP 只允许本机使用

该 Runtime 是：

```text
internal service
```

不是 LAN Server。

必须：

```text
仅本机访问
```

AIKS 不提供：

```text
0.0.0.0
LAN access
外部端口开放
```

并禁止自动启用：

```text
Publish
WebDAV
网络暴露
```

避免让个人知识库意外暴露到局域网。

---

# 17. SiYuan Runtime Manager

新增：

```rust
pub struct SiyuanRuntime {
    child: Option<Child>,
    port: u16,
    workspace: PathBuf,
    state: RuntimeState,
}
```

状态：

```rust
enum RuntimeState {
    Stopped,
    Starting,
    Ready,
    Failed,
    Stopping,
}
```

提供：

```rust
start()
stop()
restart()
health()
wait_until_ready()
```

---

# 18. Runtime Health Check

启动 SiYuan 后轮询：

```text
/api/system/version
```

或者官方启动状态 API。

超时：

```text
30 秒
```

结果：

```text
Ready
```

才允许启动 Sync Engine。

否则 UI 显示：

```text
知识引擎启动失败

[重新启动]
[打开日志]
[修复]
```

不能整个程序直接退出。

---

# 19. 生命周期

启动：

```text
AIKS Desktop
    ↓
SiYuan Runtime
    ↓
AIKS Core
    ↓
Watcher
    ↓
UI
```

退出：

```text
用户退出 AIKS
    ↓
停止 Watcher
    ↓
等待当前 Sync 完成/取消
    ↓
flush SQLite
    ↓
关闭 SiYuan
    ↓
退出
```

不能：

```text
直接 Kill SiYuan
```

优先正常退出。

若超时：

```text
5~10 秒
```

再 terminate。

---

# 20. 崩溃恢复

若：

```text
AIKS Crash
```

残留：

```text
SiYuan-Kernel.exe
```

下一次启动：

```text
检测已有 managed PID
```

如果：

```text
PID 属于 AIKS workspace
```

则：

```text
复用
或
安全关闭后重新启动
```

不得随意 kill 用户自己安装的 SiYuan。

所以保存：

```text
runtime.json
```

例如：

```json
{
  "pid": 18244,
  "port": 6811,
  "workspace": "...",
  "startedByAiks": true
}
```

---

# 21. 主窗口设计

用户双击 AIKS 后看到：

```text
┌──────────────────────────────────────────────┐
│ AIKS                              ● 已同步   │
├────────────┬─────────────────────────────────┤
│            │                                 │
│  知识库    │                                 │
│            │       Knowledge Workspace       │
│  数据来源  │                                 │
│            │                                 │
│  同步记录  │                                 │
│            │                                 │
│  设置      │                                 │
│            │                                 │
└────────────┴─────────────────────────────────┘
```

但为了 V2 稳定性，不建议把 SiYuan Web UI 强行塞进不成熟的 Multi-WebView 布局。

Tauri v2 支持 WebviewWindow；同一 Window 内的 multiwebview 当前仍属于不稳定能力，所以生产版不应把核心架构建立在 unstable multiwebview 上。

---

# 22. 推荐 UI 实现方式

采用：

```text
主窗口：
SiYuan Knowledge Workspace

辅助窗口：
AIKS Control Center
```

两者都属于：

```text
同一个 AIKS Desktop
```

不是两个程序。

用户体验：

```text
双击 AIKS
     ↓
知识库窗口
```

右上角或系统托盘：

```text
AIKS 状态
数据源
设置
同步历史
```

点击：

```text
打开 AIKS Control Center
```

弹出同一应用的设置窗口。

---

# 23. 主窗口

主窗口直接加载：

```text
http://127.0.0.1:<runtime-port>
```

也就是内置 SiYuan Web Stage。

用户看到的是完整：

```text
笔记
文档
编辑器
搜索
标签
双链
数据库
AI
```

而不知道后面运行的是 SiYuan。

窗口标题可以显示：

```text
AIKS Knowledge
```

---

# 24. Control Center

Control Center 使用：

```text
React + Vite
```

页面：

```text
概览

数据来源

同步

设置

日志

关于
```

---

# 25. 概览页

设计：

```text
AIKS

● 正常运行

知识引擎
✓ Ready

AI 数据来源
────────────────

✓ Codex             134
✓ OpenCode          518
✓ Gemini CLI          8
⚠ Claude Code         0

总 Session          660

最近同步
2026-09-11 15:30

同步状态
────────────────
Synced              652
Pending               8
Conflict              0
Failed                0

[立即同步]
```

---

# 26. 数据来源页

显示：

```text
Codex

状态：已检测
Session：134
目录：
C:\Users\...\ .codex

自动同步：ON
```

OpenCode：

```text
状态：已检测
Session：518
Database：
...\opencode.db

Schema：
opencode-sqlite-v1

自动同步：ON
```

Gemini：

```text
Session：8
```

Claude：

```text
未检测到

安装 Claude Code 后
AIKS 会自动识别
```

不应显示：

```text
ERROR
```

因为没安装是正常状态。

---

# 27. 用户不再配置路径

默认：

```text
自动发现
```

高级设置里才允许：

```text
Custom Path
```

普通用户完全不需要配置：

```text
~/.codex
~/.gemini
opencode.db
```

---

# 28. 第一次扫描

第一次打开程序：

```text
正在发现您的 AI 对话...
```

结果：

```text
发现 660 个 Session

Codex       134
Gemini        8
OpenCode    518

[开始导入]
```

推荐：

```text
默认自动开始
```

而不是必须点按钮。

---

# 29. 首次大批量导入

660 个 Session 不能阻塞 UI。

后台：

```text
Discovery
    ↓
Queue
    ↓
Parse
    ↓
Render
    ↓
SiYuan
```

UI：

```text
首次导入

327 / 660

██████████░░░░ 49%

您可以继续使用 AIKS
```

允许最小化。

---

# 30. 真正实现 File Watcher

当前项目只有：

```text
Periodic Scan
```

V2 必须补完整：

```text
notify crate
```

监控：

```text
.codex
.claude
.gemini
opencode.db
opencode.db-wal
```

工作方式：

```text
文件事件
    ↓
debounce 2s
    ↓
affected provider
    ↓
affected session
    ↓
incremental parse
    ↓
sync
```

同时保留：

```text
Periodic Scanner
```

每：

```text
5 分钟
```

兜底。

即：

```text
Watcher
+
Periodic Scan
```

两个都必须存在。

---

# 31. 同步延迟目标

普通场景：

```text
新消息
   ↓
2~5 秒
   ↓
知识库可见
```

Periodic Scanner 只是：

```text
漏事件恢复
```

不是主同步方式。

---

# 32. 修复 dry-run 逻辑

当前：

```text
sync --dry-run
    ↓
先检查 SiYuan
```

导致 SiYuan 不在线就失败。

V2 修改：

```text
dry-run
    ↓
Discovery
    ↓
Hash
    ↓
计算 NEW / UPDATED
```

不依赖 SiYuan。

Desktop UI 也不再暴露：

```text
dry-run
```

这是开发者功能。

---

# 33. 系统托盘

AIKS 必须支持 Windows Tray。

菜单：

```text
打开 AIKS

立即同步

暂停同步

打开知识库

打开数据目录

查看日志

开机自动启动 ✓

退出
```

关闭主窗口默认：

```text
最小化到托盘
```

不要直接退出。

真正退出：

```text
托盘 → 退出
```

---

# 34. 开机自动启动

设置：

```text
开机启动
```

默认建议：

```text
ON
```

但首次启动明确告诉用户。

启动后：

```text
AIKS tray
+
SiYuan Kernel
+
Watcher
```

后台自动运行。

用户无需：

```text
aiks daemon
```

---

# 35. 配置系统调整

当前：

```text
config.toml
```

保留给开发者。

Desktop 正式配置保存：

```text
app.json
```

例如：

```json
{
  "startup": true,
  "syncEnabled": true,
  "scanIntervalSeconds": 300,
  "includeThinking": false,
  "includeToolCalls": true,
  "maxToolResultChars": 10000
}
```

普通 UI 提供开关。

---

# 36. 高级设置

可以提供：

```text
同步
────────────────
☑ 实时自动同步
兜底扫描：5 分钟

内容
────────────────
☑ Tool Call
☑ Tool Result
☐ Thinking
最大 Tool Result：10000

隐私
────────────────
☑ 自动过滤 Secret

应用
────────────────
☑ 开机启动
☑ 关闭窗口后驻留后台
```

---

# 37. AIKS Doctor GUI 化

CLI：

```text
aiks doctor
```

继续保留。

Desktop 显示：

```text
系统诊断

AIKS State DB       ✓
SiYuan Kernel       ✓
SiYuan Workspace    ✓
Codex               ✓
Gemini              ✓
OpenCode            ✓
Claude              未安装

[复制诊断信息]
```

---

# 38. 用户不能看到 SiYuan Token

UI 禁止提供：

```text
SiYuan URL
SiYuan API Token
AccessAuthCode
```

普通用户不需要知道。

只在：

```text
Developer Mode
```

下显示：

```text
Kernel Port
Kernel Version
Workspace Path
```

Token 永远不明文显示。

---

# 39. Update Strategy

AIKS 与 SiYuan Runtime 必须作为一个兼容组合发布：

```text
AIKS 2.0.0

includes:

AIKS Core 2.0.0
SiYuan x.y.z
Provider Schemas:
 Claude x
 Codex x
 Gemini x
 OpenCode x
```

不能：

```text
SiYuan 自动升级
```

因为上游升级可能导致：

```text
API breaking
resource breaking
security behavior changes
```

SiYuan Runtime 升级必须跟随：

```text
AIKS Release
```

一起测试和发布。

---

# 40. 软件升级

用户看到：

```text
AIKS 有新版本

2.0.0 → 2.1.0

[立即更新]
```

安装包内部同时更新：

```text
Desktop
Core
SiYuan Runtime
```

但绝不能覆盖：

```text
workspace
aiks.db
archive
settings
```

---

# 41. 备份

设置中：

```text
数据与备份
```

提供：

```text
打开数据目录

导出备份

恢复备份
```

至少备份：

```text
siyuan/workspace
aiks.db
archive/
config/
```

---

# 42. 卸载

卸载时不能默认删除知识。

卸载程序：

```text
是否同时删除个人知识库数据？

☐ 删除个人数据
```

默认：

```text
不勾选
```

因此：

```text
重新安装 AIKS
```

可以恢复全部数据。

---

# 43. Knowledge Extractor

V2 Desktop 可以先保留：

```text
关闭
```

不要因为桌面化顺便扩大范围。

当前核心优先级仍然是：

```text
稳定采集
+
稳定同步
+
单应用体验
```

Knowledge Extractor：

```text
V2.1 / V2.5
```

再实现。

---

# 44. CLI 保留

桌面化后不要删除：

```text
aiks-cli.exe
```

它作为：

```text
Developer / Troubleshooting Tool
```

保留：

```text
doctor
scan
sync
status
resync
rebuild-state
```

但安装用户不需要使用。

---

# 45. Build Pipeline

Windows Release：

```text
Build aiks-core
      ↓
Build aiks-cli
      ↓
Build React
      ↓
Validate SiYuan Runtime
      ↓
Bundle sidecar/resources
      ↓
Build Tauri
      ↓
Sign
      ↓
AIKS-Setup-x64.exe
```

---

# 46. SiYuan Sidecar 打包

使用 Tauri：

```text
bundle.externalBin
```

注册：

```text
SiYuan-Kernel
```

Tauri 官方 sidecar 机制支持把外部可执行文件包含在应用包中并从 Rust 端启动。

目录资源：

```text
stage
appearance
guide
```

通过 Tauri resources 打包。

---

# 47. 发布前 SiYuan 验证

CI 必须：

```text
启动 bundled SiYuan
```

然后自动测试：

```text
Version
Startup Progress
Create Notebook
Create Document
Update Block
Set Attributes
Upload Asset
Search
Shutdown
```

全部成功才能 Release。

---

# 48. License

必须特别注意。

SiYuan 当前采用 AGPL-3.0。

AIKS 如果把 SiYuan Kernel 和其 Web 资源直接随安装包重新分发：

```text
属于对 SiYuan 二进制/资源的再分发
```

因此必须：

```text
保留 LICENSE

保留 Copyright

THIRD_PARTY_NOTICES

明确 bundled SiYuan version

提供对应源码获取方式

遵守 AGPL 要求
```

发布商业版本前应单独做一次 License / 法务确认。

架构上仍保持：

```text
AIKS
   │ HTTP
   ▼
SiYuan Kernel
```

不要修改 SiYuan Core，尽量保持清晰的独立组件边界。

SiYuan 官方仓库为独立开源项目，AIKS 不应删除、隐藏其 License 信息。

---

# 49. 第三方信息页面

AIKS：

```text
设置
→ 关于
→ 开源组件
```

显示：

```text
SiYuan
Copyright ...
AGPL-3.0
Version ...
Source:
https://github.com/siyuan-note/siyuan

AICoder Session Viewer
MIT

CC Switch
MIT

ccusage
MIT
```

---

# 50. 现有 V1 缺口在本阶段全部处理

当前已知：

```text
File Watcher 未完成
```

V2：

```text
必须完成
```

当前：

```text
dry-run 依赖 SiYuan
```

V2：

```text
修正
```

当前：

```text
用户需要启动 SiYuan
```

V2：

```text
取消
```

当前：

```text
用户需要 SIYUAN_TOKEN
```

V2：

```text
取消
```

当前：

```text
命令行启动 daemon
```

V2：

```text
取消
```

当前：

```text
没有 UI
```

V2：

```text
Tauri Desktop + Tray
```

---

# 51. 开发阶段

## Phase D1 — Core Library 化

完成：

```text
aiks-core
aiks-cli
```

所有 47 个现有测试：

```text
必须继续通过
```

---

## Phase D2 — Tauri Desktop Skeleton

建立：

```text
Tauri 2
React
Vite
TypeScript
```

完成：

```text
主窗口
Control Center
Tray
```

---

## Phase D3 — Embedded SiYuan

完成：

```text
打包 Kernel
打包 stage
打包 appearance
打包 guide

Runtime Manager
Port Allocator
Workspace
Health Check
Shutdown
```

---

## Phase D4 — Zero Config Bootstrap

完成：

```text
自动 Workspace
自动 Token
自动 Notebook
自动 Source Discovery
```

首次启动：

```text
零人工配置
```

---

## Phase D5 — Desktop ↔ Core

Tauri command：

```text
get_status
scan_sources
sync_now
get_sessions
get_sync_history
get_settings
save_settings
open_data_folder
restart_siyuan
```

---

## Phase D6 — Real Watcher

真正接入：

```text
notify
```

完成：

```text
Codex
Claude
Gemini
OpenCode DB/WAL
```

事件监听。

---

## Phase D7 — UX

完成：

```text
First Run
Import Progress
Provider Status
Sync Status
Conflict
Error Recovery
Tray
Auto Start
```

---

## Phase D8 — Packaging

完成：

```text
Installer
Upgrade
Uninstall
Data Preserve
License
```

---

# 52. V2 验收标准

测试机器：

```text
不安装 SiYuan
不安装 Rust
不安装 Node
不设置任何环境变量
```

安装：

```text
AIKS-Setup-x64.exe
```

然后：

```text
双击 AIKS
```

必须自动：

```text
✓ 启动知识引擎

✓ 识别 Codex

✓ 识别 Gemini

✓ 识别 OpenCode

✓ 创建 Knowledge Workspace

✓ 导入已有 Session

✓ 显示知识内容

✓ 支持搜索

✓ 支持编辑

✓ 支持新增人工笔记

✓ 新 Session 自动进入知识库

✓ 关闭窗口后后台同步

✓ 重启电脑后可自动启动
```

用户整个过程：

```text
0 条命令
0 个 Token
0 个端口
0 个配置文件
0 次单独启动 SiYuan
```

这才算 V2 完成。

---

# 53. 最终用户体验

第一次：

```text
下载 AIKS
      ↓
安装
      ↓
打开
      ↓

正在初始化您的个人 AI 知识库...

发现：

Codex       134
Gemini        8
OpenCode    518

正在导入历史知识...
```

进入：

```text
AIKS Knowledge
```

以后：

```text
用户使用 Codex / OpenCode / Gemini

           ↓

       自动采集

           ↓

       自动同步

           ↓

     AIKS Knowledge
```

用户完全不需要意识到：

```text
SiYuan
SQLite
JSONL
WAL
Embedding
Renderer
HTTP API
```

这些技术组件存在。

---

# 54. 给 Coding Agent 的最终强制指令

> 当前 AIKS V1 Core 已经实现并验证，不允许重写 Provider、Canonical Model、Sync Engine 等成熟模块。

> 第一任务是把现有 Rust 项目拆为 `aiks-core + aiks-cli`，保证所有现有测试继续通过。

> 然后基于 Tauri 2 建设 AIKS Desktop。

> 最终产品必须是一个独立安装程序。用户不得单独安装或启动 SiYuan。

> SiYuan 必须作为 bundled sidecar/runtime 由 AIKS 生命周期管理。

> 不得要求普通用户配置 `SIYUAN_TOKEN`、SiYuan URL、端口、workspace 或 config.toml。

> 所有 SiYuan 鉴权和 workspace bootstrap 必须由程序自动完成。

> 用户主界面直接提供完整知识管理能力。

> 当前 V1 的 periodic scan 必须保留，同时补齐真正的 filesystem watcher。

> 开发完成后必须在一台“从未安装过 SiYuan”的干净 Windows 11 虚拟机执行完整 E2E 测试。

> 只有在干净 Windows 机器上实现“安装 → 双击 → 自动发现 Session → 自动导入 → 可视化查看知识”全流程无人工配置，才允许标记 V2 完成。

---

# 55. 最终架构定义

V1：

```text
AIKS
+
用户安装的 SiYuan
```

正式废弃。

V2：

```text
                    AIKS Desktop
                         │
             ┌───────────┴────────────┐
             │                        │
          AIKS Core             Knowledge Engine
             │                        │
     Session Collector          Embedded SiYuan
             │                        │
     ┌───────┼───────┐                │
     │       │       │                │
   Codex  Gemini  OpenCode            │
     │             Claude             │
     └──────────┬─────────────┬───────┘
                │             │
                ▼             ▼
             History       Knowledge
                              │
                    Search / Edit / AI
```

用户眼里只有：

```text
AIKS
```

这就是最终产品。