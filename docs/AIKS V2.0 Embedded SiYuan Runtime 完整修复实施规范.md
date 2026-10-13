# AIKS V2.0 Embedded SiYuan Runtime 完整修复实施规范

版本：V2.0-Runtime-Fix-1  
适用项目：AI Knowledge Sync / AIKS Desktop  
目标平台：Windows 11 x64  
目标：修复当前 Embedded SiYuan Runtime 集成，使 AIKS Desktop 真正达到“安装即用、打开即用、无需单独安装或配置 SiYuan”

---

# 1. 修复背景

当前 AIKS V2 已经完成：

- `aiks-core`
- `aiks-cli`
- `aiks-desktop`
- Claude / Codex / Gemini / OpenCode Provider
- Canonical Model
- SQLite State
- Sync Engine
- Markdown Renderer
- File Watcher
- Tauri Desktop
- System Tray
- SiYuan Runtime Manager
- Bootstrap
- Lifecycle
- React Control Center

现有核心代码无需推翻。

当前发现的问题集中在：

> Embedded SiYuan Runtime 的准备、版本锁定、启动、鉴权和打包链路。

当前第一次执行：

```powershell
.\scripts\setup-siyuan.ps1
```

脚本尝试下载：

```text
siyuan-3.1.31-win.zip
```

但官方 Release 并不存在该文件。

官方 `v3.1.31` Release 存在，但 Windows 发布文件实际为：

```text
siyuan-3.1.31-win.exe
siyuan-3.1.31-win-arm64.exe
```

而不是 ZIP。

同时，AIKS V2 当前 Runtime 启动方案使用：

```text
SiYuan-Kernel.exe serve ...
```

而 SiYuan 从 v3.7.0 起才正式要求通过 `serve` 子命令启动服务器。

因此：

```text
3.1.31
```

虽然真实存在，但不适合作为当前 AIKS V2 Runtime 的固定版本。

---

# 2. 本次修复核心结论

本次不只修改：

```text
scripts/setup-siyuan.ps1
```

而是完整检查和修复以下链路：

```text
SiYuan Version Lock
        ↓
Release Asset Resolution
        ↓
Runtime Extraction
        ↓
Runtime Layout
        ↓
Kernel Startup
        ↓
Workspace Bootstrap
        ↓
Authentication Strategy
        ↓
SiYuan Sink
        ↓
Tauri Resource Bundle
        ↓
Development Startup
        ↓
Release Build
        ↓
Installer
        ↓
Clean Windows E2E
```

---

# 3. 本次禁止修改的核心模块

除非测试发现真正 Bug，否则不得重新设计以下模块：

```text
providers/
model/
model/hash.rs
renderer/
storage/
sync/
watcher/
archive/
sanitizer/
Canonical Model
Provider Registry
OpenCode Parser
Codex Parser
Gemini Parser
Claude Parser
State SQLite
Incremental Scanner
```

尤其不得借本次修复重新实现：

```text
四种 Provider
Session 解析
Markdown Renderer
State DB
Sync Engine
```

本次修改范围必须集中于：

```text
Embedded SiYuan Runtime
+
Desktop Integration
```

---

# 4. 固定 SiYuan 版本

当前首次修复固定：

```text
SiYuan 3.8.3
```

Windows x64 官方 Release Asset：

```text
siyuan-3.8.3-win.exe
```

SHA256：

```text
49cd67c6e892aff5a04189707007df3046d48c0dc279840433b9824edd374569
```

严禁：

```text
动态使用 latest
```

严禁：

```text
每次构建自动选择 GitHub 最新版
```

AIKS 与 SiYuan 必须形成经过验证的兼容组合：

```text
AIKS x.y.z
    +
SiYuan 3.8.3
```

未来升级到 3.8.4 / 3.9.x：

必须显式修改版本锁定并重新运行 E2E。

---

# 5. 修改 siyuan.version

当前 `siyuan.version` 应改成明确的 Key/Value 格式。

建议：

```text
version=3.8.3
release_tag=v3.8.3
asset=siyuan-3.8.3-win.exe
platform=windows-x64
sha256=49cd67c6e892aff5a04189707007df3046d48c0dc279840433b9824edd374569
```

不得只保存：

```text
version=3.8.3
```

因为这样 setup 脚本又必须猜：

```text
扩展名
架构
Asset Name
```

Asset Name 必须由版本锁定文件明确指定。

---

# 6. 不再手工拼 Release 文件名

禁止：

```text
"siyuan-$version-win.zip"
```

禁止：

```text
猜测 GitHub 文件名
```

正确流程：

```text
读取 siyuan.version
        ↓
version
release_tag
asset
sha256
        ↓
查询 siyuan-note/siyuan 对应 Release
        ↓
精确寻找：

asset.name == 配置中的 asset

        ↓
读取 browser_download_url
        ↓
下载
```

这样：

```text
GitHub Release 不存在
```

或者：

```text
Asset Name 不存在
```

应立即报明确错误：

```text
SiYuan release asset not found:
tag=v3.8.3
asset=siyuan-3.8.3-win.exe
```

而不是产生一个错误 URL 后等待 404。

---

# 7. scripts/setup-siyuan.ps1 必须完全重构

脚本必须兼容：

```text
Windows PowerShell 5.1
```

不能强制：

```text
pwsh
PowerShell 7
```

用户已经验证机器上可能只有：

```text
powershell.exe
```

因此所有脚本必须在 Windows PowerShell 5.1 下正常执行。

---

# 8. PowerShell 项目路径修复

所有 scripts 下脚本统一使用：

```powershell
$ErrorActionPreference = "Stop"

$ScriptDir = $PSScriptRoot
$ProjectRoot = Split-Path $ScriptDir -Parent
```

以后：

```powershell
Join-Path $ProjectRoot "siyuan.version"
```

而不是：

```powershell
Join-Path $PSScriptRoot "siyuan.version"
```

必须检查：

```text
setup-siyuan.ps1
dev.ps1
build-release.ps1
```

三者是否都有类似错误。

---

# 9. setup-siyuan.ps1 的完整职责

新的 setup script 必须完成：

```text
1. 找到 ProjectRoot

2. 读取 siyuan.version

3. 验证必需字段

4. 查询官方 Release

5. 验证指定 Asset 确实存在

6. 下载官方 Windows Installer

7. SHA256 校验

8. 获取 SiYuan Runtime 文件

9. 建立标准 Runtime 目录

10. 验证 Kernel 文件

11. 验证 stage

12. 验证 appearance

13. 执行 Kernel --help 测试

14. 执行 serve smoke test

15. 调用 version API

16. 停止临时 Kernel

17. 清理临时目录

18. 输出 Runtime Ready
```

---

# 10. Setup 缓存

不要每次重新下载 200MB 左右的 SiYuan。

建议：

```text
.build/
└── cache/
    └── siyuan/
        └── 3.8.3/
            └── siyuan-3.8.3-win.exe
```

每次运行：

```text
文件存在
    ↓
计算 SHA256
    ↓
正确
    ↓
直接复用
```

如果 Hash 错误：

```text
删除
↓
重新下载
```

---

# 11. SHA256 强制校验

下载完成后：

```powershell
Get-FileHash
```

必须与：

```text
siyuan.version.sha256
```

完全一致。

不一致：

```text
立即停止构建
```

不得：

```text
WARN 后继续
```

错误：

```text
SiYuan checksum verification failed.

Expected:
49cd...

Actual:
xxxx...

The downloaded runtime will not be used.
```

---

# 12. 不再使用 Expand-Archive

官方 Windows Release 是：

```text
NSIS Installer EXE
```

不是 ZIP。

因此删除：

```powershell
Expand-Archive $zipPath
```

以及：

```text
$zipUrl
$zipPath
```

等旧命名。

统一改为：

```text
$installerPath
```

---

# 13. NSIS Runtime 获取策略

setup 脚本需要从官方 Windows Installer 中获取运行资源。

推荐支持两个模式。

## 模式 A：已有 7-Zip

如果系统检测到：

```text
7z.exe
```

允许使用 7-Zip 将 NSIS Installer 解包到 staging。

然后从 staging 中定位：

```text
resources/kernel/
resources/stage/
resources/appearance/
resources/guide/
```

---

## 模式 B：没有 7-Zip

不能因为没有 7-Zip 失败。

使用官方 NSIS silent install：

```text
/S
/D=<isolated staging directory>
```

安装到临时 staging：

```text
.build/staging/siyuan-3.8.3/
```

只将其作为：

```text
构建期 Runtime Source
```

然后复制需要的 Runtime 文件。

完成后清理 staging。

注意：

```text
/D 参数必须是最后一个 NSIS 参数
```

脚本必须等待安装进程退出并检查：

```text
ExitCode
```

---

# 14. 最终 Runtime 目录不要拆散

之前设计：

```text
binaries/siyuan/
resources/siyuan/
```

会让 Working Directory 管理更加复杂。

本次建议统一改成：

```text
apps/
└── aiks-desktop/
    └── src-tauri/
        └── resources/
            └── siyuan/
                ├── kernel/
                │   └── SiYuan-Kernel.exe
                │
                ├── stage/
                │
                ├── appearance/
                │
                ├── guide/
                │
                ├── changelogs/
                │
                └── ...
```

核心原则：

> 尽可能保持 SiYuan 官方 `resources` 运行目录结构。

原因：

SiYuan Kernel 默认 Working Directory 逻辑本身就是围绕：

```text
resources/
├── kernel/
├── stage/
└── appearance/
```

设计。

不要为了 Tauri 人为把 Kernel 和 stage 拆到完全不同目录。

---

# 15. 推荐取消 SiYuan externalBin

本项目中的 SiYuan Kernel 并不需要作为 Tauri Shell Sidecar 暴露给前端。

Runtime 生命周期全部由 Rust：

```text
aiks-core/runtime
```

控制。

因此更推荐：

```text
SiYuan Runtime
=
Tauri Resource
```

整个：

```text
resources/siyuan/
```

随安装包打包。

Rust 通过：

```text
std::process::Command
```

启动：

```text
<resource_dir>/siyuan/kernel/SiYuan-Kernel.exe
```

这样无需：

```text
externalBin target triple rename
sidecar binary naming
```

减少打包复杂度。

---

# 16. tauri.conf.json 修改

确保：

```text
resources/siyuan/**
```

被打进 Release。

不得只打：

```text
SiYuan-Kernel.exe
```

因为 Kernel 需要：

```text
stage
appearance
guide
```

等资源。

Release 打包前必须检查。

---

# 17. Runtime 文件完整性检查

setup 完成后至少检查：

```text
resources/siyuan/kernel/SiYuan-Kernel.exe
resources/siyuan/stage/
resources/siyuan/appearance/
resources/siyuan/guide/
```

至少：

```text
kernel exists
kernel size > 1MB

stage exists
stage contains files

appearance exists
appearance contains files
```

否则：

```text
setup FAILED
```

---

# 18. Runtime Manager 路径修改

修改：

```text
crates/aiks-core/src/runtime/mod.rs
```

或者当前实际 Runtime Manager 所在文件。

禁止硬编码开发路径：

```text
apps/aiks-desktop/src-tauri/...
```

Runtime Manager 接受：

```rust
pub struct SiyuanRuntimeConfig {
    pub runtime_root: PathBuf,
    pub workspace: PathBuf,
    pub port: u16,
}
```

Desktop Bootstrap 负责确定：

```text
runtime_root
```

Core 不知道：

```text
Tauri
src-tauri
安装目录
```

---

# 19. Development Runtime 路径

开发模式：

```text
<ProjectRoot>/
apps/aiks-desktop/src-tauri/resources/siyuan
```

Release：

通过 Tauri：

```text
AppHandle.path().resource_dir()
```

解析：

```text
<resource-dir>/siyuan
```

两种路径最终都传给：

```text
SiyuanRuntime
```

---

# 20. 正确 Kernel 路径

统一：

```text
runtime_root/
└── kernel/
    └── SiYuan-Kernel.exe
```

即：

```rust
runtime_root
    .join("kernel")
    .join("SiYuan-Kernel.exe")
```

---

# 21. SiYuan 3.8.3 正确启动形式

使用：

```text
SiYuan-Kernel.exe serve
```

启动参数至少：

```text
serve
--workspace=<AIKS workspace>
--wd=<runtime_root>
--port=<allocated port>
--lang=zh-CN
--mode=prod
```

不要传：

```text
--attach-ui
```

因为：

```text
AIKS
```

并不是 SiYuan 官方 Electron UI。

AIKS 自己负责 Kernel 生命周期。

---

# 22. --wd 必须明确正确

`--wd` 应指向：

```text
runtime_root
```

例如：

```text
...\resources\siyuan
```

目录里面应该直接存在：

```text
kernel/
stage/
appearance/
guide/
```

不能设置成：

```text
...\resources\siyuan\kernel
```

否则：

```text
stage
appearance
```

解析路径会错误。

---

# 23. Workspace

用户数据继续保持：

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
├── logs/
└── config/
```

SiYuan Workspace：

```text
%LOCALAPPDATA%\AIKnowledgeSync\siyuan\workspace
```

不得放：

```text
Program Files
安装目录
项目源码目录
```

---

# 24. Port Allocation

不要固定：

```text
6806
```

启动前：

```text
6806
↓
检测
↓
占用则 6807
↓
...
```

建议最大尝试：

```text
6806 ～ 6899
```

找到空闲端口。

或者使用系统随机端口。

如果使用随机端口：

Runtime Manager 必须明确知道最终端口。

第一版推荐：

```text
AIKS 自己分配一个明确空闲端口
```

然后通过：

```text
--port
```

传给 Kernel。

---

# 25. Security Strategy 重新简化

当前 Embedded Runtime 是：

```text
AIKS 专用
本机专用
不提供 LAN 服务
```

因此 V2.0 建议：

> 不设置 SiYuan Lock Screen Password。

即：

```text
不要传 --accessAuthCode
```

同时：

```text
不要要求用户配置 SIYUAN_TOKEN
```

---

# 26. 为什么可以不设置 AccessAuthCode

SiYuan 当前 Kernel 行为：

```text
NetworkServe = false
```

时监听：

```text
127.0.0.1
```

而不是：

```text
0.0.0.0
```

且未配置 AccessAuthCode 时，本机请求可以直接通过授权。

这非常适合：

```text
Embedded Desktop Application
```

因为：

```text
AIKS Desktop
    ↓
localhost
    ↓
SiYuan Kernel
```

不需要：

```text
登录页面
Cookie Bootstrap
API Token Bootstrap
Credential Manager
```

这比之前“随机 accessAuthCode + 自动 Token”的方案简单且稳定得多。

---

# 27. 强制确保 NetworkServe=false

由于不设置 AccessAuthCode：

必须确保：

```text
SiYuan
```

只监听 loopback。

Bootstrap / Runtime Health 必须检查：

```text
127.0.0.1:<port>
```

正常。

不能主动打开：

```text
0.0.0.0
```

不得启用：

```text
NetworkServe
Publish
LAN Mode
```

AIKS 专用 Workspace 必须保持：

```text
networkServe=false
```

---

# 28. API Token 改为可选

当前：

```text
sink/siyuan.rs
```

如果强制：

```text
Authorization: Token <token>
```

需要修改。

配置改为：

```rust
pub struct SiyuanClientConfig {
    pub base_url: String,
    pub token: Option<String>,
}
```

发送请求：

```text
token Some
    → 添加 Authorization

token None
    → 不添加 Authorization
```

AIKS Embedded Mode：

```text
token = None
```

Legacy External SiYuan Mode 如果未来仍保留：

```text
token = Some(...)
```

---

# 29. 保留 External SiYuan Developer Override

为了调试可以保留：

```text
SIYUAN_URL
SIYUAN_TOKEN
```

但只能作为：

```text
Developer Override
```

普通桌面用户永远不需要配置。

优先级：

```text
Developer override configured
        ↓
使用 external runtime

否则
        ↓
使用 embedded runtime
```

Release 默认：

```text
embedded
```

---

# 30. Kernel Health Check

启动后不能仅检查：

```text
TCP port open
```

必须调用：

```text
GET /api/system/version
```

只有返回：

```text
HTTP 200
+
合法 SiYuan version
```

才进入：

```text
RuntimeState::Ready
```

---

# 31. Version Check

Health Response 得到：

```text
3.8.3
```

必须与：

```text
siyuan.version
```

匹配。

例如：

```text
Expected 3.8.3
Actual 3.7.9
```

输出：

```text
Bundled SiYuan runtime version mismatch
```

不能静默运行错误 Runtime。

---

# 32. Startup Timeout

启动最多：

```text
30 秒
```

轮询建议：

```text
250ms → 500ms → 1s
```

如果 Kernel 进程已经退出：

不继续等 30 秒。

立即读取：

```text
stdout
stderr
exit code
```

返回错误。

---

# 33. Kernel 日志

SiYuan stdout / stderr 必须进入：

```text
%LOCALAPPDATA%\AIKnowledgeSync\logs\
```

例如：

```text
siyuan-2026-09-11.log
```

不要直接丢弃。

UI：

```text
日志
```

页可查看最后若干行。

---

# 34. Runtime State

保持：

```rust
enum RuntimeState {
    Stopped,
    Starting,
    Ready,
    Failed,
    Stopping,
}
```

并增加：

```text
pid
port
version
runtime_root
workspace
last_error
```

---

# 35. Graceful Shutdown

退出 AIKS：

首先调用 SiYuan 官方：

```text
/api/system/exit
```

如果成功：

等待进程退出。

例如：

```text
最多 8 秒
```

超时：

```text
Child.kill()
```

不要默认直接：

```text
taskkill /F
```

---

# 36. 崩溃恢复

保存：

```text
runtime.json
```

内容：

```json
{
  "pid": 12345,
  "port": 6812,
  "workspace": "...",
  "version": "3.8.3",
  "startedByAiks": true
}
```

下次启动：

```text
检查 PID
↓
检查进程名
↓
检查 workspace
↓
检查 version endpoint
```

只有明确属于 AIKS Runtime 才允许操作。

禁止 Kill：

```text
用户自己安装的 SiYuan
```

---

# 37. Bootstrap 修改

修改：

```text
crates/aiks-core/src/bootstrap/
```

以及：

```text
apps/aiks-desktop/src-tauri/src/bootstrap.rs
```

根据实际代码归属避免重复逻辑。

启动顺序：

```text
1. resolve app data

2. create directories

3. open AIKS DB

4. resolve embedded SiYuan runtime

5. validate runtime

6. allocate port

7. start Kernel

8. wait version endpoint

9. initialize SiYuan Sink

10. ensure Notebook

11. initialize Providers

12. start Watcher

13. perform background initial sync
```

---

# 38. 不再需要 Token Bootstrap

删除或禁用普通运行路径中的：

```text
读取 SIYUAN_TOKEN
要求用户配置 Token
API Token 初始化
随机 accessAuthCode
自动登录 SiYuan
```

Embedded Mode 不需要。

如果这些代码已经存在：

不要粗暴删除历史兼容。

封装为：

```text
ExternalSiyuanMode
```

即可。

---

# 39. Desktop Lifecycle

修改：

```text
apps/aiks-desktop/src-tauri/src/lifecycle.rs
```

成功：

```text
Bootstrap
↓
Kernel Ready
↓
Control Center Ready
↓
Knowledge Window Ready
↓
Background Sync
```

Kernel 失败：

Control Center 仍然必须能打开。

显示：

```text
知识引擎启动失败

[重新启动]
[重新准备运行环境]
[打开日志]
```

禁止：

```text
Desktop 直接退出
```

---

# 40. Knowledge Window

Knowledge Window URL：

```text
http://127.0.0.1:<port>
```

由于 Embedded Mode 不设置 AccessAuthCode：

应该直接进入：

```text
SiYuan Stage
```

不能出现：

```text
锁屏密码
Token
登录页面
```

如果出现：

视为 Bootstrap 配置错误。

---

# 41. Control Center

Control Center 继续使用现有 React UI。

无需重做。

只增加 Runtime 状态：

```text
知识引擎

状态：
● Ready

版本：
3.8.3

运行模式：
Embedded

Port：
6812
```

Port 只在：

```text
Developer Information
```

区域显示。

普通主页不需要显示端口。

---

# 42. 设置页

删除普通用户设置中的：

```text
SiYuan URL
SiYuan Token
SiYuan Port
```

普通用户只能看到：

```text
数据目录
自动同步
开机启动
关闭窗口驻留后台
同步周期
Thinking
Tool Call
Tool Result
Secret Sanitizer
```

---

# 43. scripts/dev.ps1 必须修改

开发启动：

```text
1. 检查 Runtime

2. Runtime 不存在
   → 提示运行 setup-siyuan.ps1
   或自动调用 setup

3. 检查 npm dependencies

4. 启动 Tauri dev
```

不要：

```text
单独启动 SiYuan
```

因为：

```text
Desktop Lifecycle
```

本身就应该启动 Kernel。

否则开发模式与 Release 行为不同。

---

# 44. 推荐 dev.ps1 行为

```text
Runtime Ready?
   │
   ├─ No → setup-siyuan
   │
   └─ Yes
         ↓
npm install / npm ci
         ↓
npm run tauri dev
```

用户最终只执行：

```powershell
.\scripts\dev.ps1
```

即可。

---

# 45. scripts/build-release.ps1 必须修改

构建流程：

```text
Read siyuan.version
        ↓
Validate Runtime
        ↓
如果 Runtime 不存在：
调用 setup-siyuan
        ↓
验证 SHA/version
        ↓
cargo test --workspace
        ↓
frontend build
        ↓
Tauri build
        ↓
检查 NSIS Installer
```

Release 不允许：

```text
Runtime Missing
```

时继续。

---

# 46. Release Runtime Validation

打包前：

```text
Kernel executable           PASS
stage                       PASS
appearance                  PASS
guide                       PASS
Kernel --help               PASS
serve --help                PASS
temporary boot              PASS
/api/system/version=3.8.3   PASS
```

任何失败：

```text
Release Build FAILED
```

---

# 47. Runtime 不允许在用户电脑首次启动时下载

这是非常重要的要求。

错误方案：

```text
用户安装 AIKS
↓
第一次打开
↓
去 GitHub 下载 SiYuan
```

禁止。

正确方案：

```text
Developer / CI
↓
setup-siyuan.ps1
↓
准备 Runtime
↓
build-release
↓
Runtime 被打进 AIKS Installer
```

最终用户：

```text
安装 AIKS
↓
Runtime 已存在
↓
直接启动
```

真正做到：

```text
打开即用
```

---

# 48. Installer 验收

最终：

```text
AIKS-Setup-x64.exe
```

安装目录必须包含 Runtime。

但用户不需要看到或启动：

```text
SiYuan Installer
```

AIKS 安装过程中：

```text
不得再次弹出 SiYuan 安装界面
```

因为我们打包的是：

```text
已经准备好的 Runtime
```

而不是把：

```text
siyuan-3.8.3-win.exe
```

留给最终用户执行。

---

# 49. THIRD_PARTY_NOTICES 更新

记录：

```text
SiYuan
Version: 3.8.3
License: AGPL-3.0
Usage: Bundled runtime / knowledge engine
Source repository: siyuan-note/siyuan
```

并保留：

```text
LICENSE
Copyright
Source information
```

Release 中必须包含相关第三方许可。

---

# 50. About 页面

显示：

```text
AIKS

Embedded components:

SiYuan 3.8.3
AGPL-3.0

AICoder Session Viewer
MIT

CC Switch
MIT

ccusage
MIT
```

不得隐藏：

```text
SiYuan
```

作为开源依赖的事实。

---

# 51. setup-siyuan.ps1 幂等性

第一次：

```text
Runtime 不存在
↓
下载
↓
准备
↓
验证
```

第二次：

```text
Runtime 已存在
↓
Kernel version == 3.8.3
↓
文件完整
↓
直接输出：

SiYuan runtime already ready.
```

提供：

```text
-Force
```

可重新准备。

例如：

```powershell
.\scripts\setup-siyuan.ps1 -Force
```

---

# 52. Setup 最终输出

成功后建议：

```text
=== AIKS Embedded SiYuan Setup ===

Version:        3.8.3
Platform:       windows-x64
Installer:      verified
SHA256:         OK

Runtime:
  Kernel        OK
  Stage         OK
  Appearance    OK
  Guide         OK

Smoke Test:
  serve --help  OK
  Kernel boot   OK
  HTTP API      OK
  Version       3.8.3

Embedded SiYuan runtime is ready.
```

---

# 53. 增加 runtime doctor

现有：

```text
aiks doctor
```

增加：

```text
Embedded SiYuan Runtime

Runtime root: OK
Kernel:       OK
Stage:        OK
Appearance:   OK
Version:      3.8.3
Workspace:    OK
```

CLI 不一定真正启动 Kernel。

可以区分：

```text
Static Runtime Check
```

与：

```text
Live Health Check
```

---

# 54. 单元测试新增

至少增加：

```text
runtime_root_resolution
kernel_path_resolution
runtime_version_parsing
version_mismatch
missing_kernel
missing_stage
missing_appearance
port_allocation
runtime_command_args
embedded_sink_without_token
```

---

# 55. Integration Test

新增：

```text
siyuan_embedded_boot
```

测试：

```text
临时 Workspace
↓
启动 bundled Kernel
↓
等待 version
↓
创建 Notebook
↓
创建 Document
↓
更新 Document
↓
查询 Document
↓
shutdown
```

---

# 56. Desktop E2E

必须在：

```text
没有安装 SiYuan
```

的 Windows 机器执行。

同时：

```text
没有 SIYUAN_TOKEN
没有 SiYuan 配置
没有 SiYuan Workspace
```

步骤：

```text
安装 AIKS
↓
打开 AIKS
```

必须自动：

```text
启动 Kernel
创建 Workspace
创建 Notebook
扫描 Session
导入 Session
打开 Knowledge UI
```

---

# 57. E2E Session 验证

当前测试机器已经有：

```text
Codex      134
Gemini       8
OpenCode   518
```

约：

```text
660 Sessions
```

应选择一个已知 Session：

```text
ses_f717
```

同步后：

```text
Knowledge Window
```

中能够找到。

验证：

```text
Title
User messages
Assistant messages
Tool calls
Project
Source
Session ID
```

正常。

---

# 58. E2E 实时监听

程序启动后：

```text
打开 OpenCode
↓
新增一条 Session Message
```

目标：

```text
2~5 秒
```

AIKS：

```text
Watcher Event
↓
Parser
↓
Sync
↓
SiYuan Document Update
```

必须验证。

---

# 59. Periodic Scanner 仍保留

实时 Watcher 完成后不能删除：

```text
5 分钟 Periodic Scan
```

最终：

```text
Watcher
+
Periodic Scanner
```

两层。

---

# 60. dry-run 修复继续保留

确保：

```text
aiks sync --dry-run
```

不依赖：

```text
Embedded SiYuan Kernel
```

Dry Run 只计算：

```text
DISCOVERED
NEW
UPDATED
UNCHANGED
```

不得因 Kernel 未启动而失败。

---

# 61. 修改文件清单

本次至少审计以下文件。

必须修改：

```text
siyuan.version

scripts/setup-siyuan.ps1
scripts/dev.ps1
scripts/build-release.ps1

crates/aiks-core/src/runtime/mod.rs
crates/aiks-core/src/bootstrap/mod.rs
crates/aiks-core/src/sink/siyuan.rs

apps/aiks-desktop/src-tauri/tauri.conf.json
apps/aiks-desktop/src-tauri/src/bootstrap.rs
apps/aiks-desktop/src-tauri/src/lifecycle.rs

THIRD_PARTY_NOTICES.md
```

根据实际实现可能修改：

```text
apps/aiks-desktop/src-tauri/src/app_state.rs
apps/aiks-desktop/src-tauri/src/commands.rs
apps/aiks-desktop/src-tauri/src/tray.rs

apps/aiks-desktop/src/pages/OverviewPage.tsx
apps/aiks-desktop/src/pages/SettingsPage.tsx
apps/aiks-desktop/src/pages/AboutPage.tsx

README.md
docs/implementation/*
```

---

# 62. 不要出现双 Runtime 管理器

当前项目可能同时存在：

```text
aiks-core/runtime
```

和：

```text
desktop/lifecycle/bootstrap
```

实现时必须明确边界。

推荐：

```text
aiks-core

SiyuanRuntime
RuntimeConfig
RuntimeState
start
stop
health


Desktop

resolve_resource_dir
AppHandle integration
UI events
window lifecycle
```

禁止：

```text
Desktop 写一套 Process::Command

Core 又写一套 Process::Command
```

只能有一个真正 Kernel 启动实现。

---

# 63. Runtime API 推荐

最终：

```rust
pub struct SiyuanRuntimeConfig {
    pub runtime_root: PathBuf,
    pub workspace: PathBuf,
    pub port: Option<u16>,
    pub language: String,
}

pub struct SiyuanRuntimeInfo {
    pub pid: u32,
    pub port: u16,
    pub version: String,
    pub base_url: String,
}

impl SiyuanRuntime {
    pub async fn validate(
        config: &SiyuanRuntimeConfig
    ) -> Result<RuntimeValidation>;

    pub async fn start(
        config: SiyuanRuntimeConfig
    ) -> Result<SiyuanRuntimeInfo>;

    pub async fn health(
        &self
    ) -> Result<RuntimeHealth>;

    pub async fn stop(
        &mut self
    ) -> Result<()>;
}
```

---

# 64. SiyuanClient 推荐

```rust
pub struct SiyuanClient {
    base_url: String,
    token: Option<String>,
}
```

构建：

```rust
SiyuanClient::embedded(base_url)
```

等价：

```text
token=None
```

外部模式：

```rust
SiyuanClient::external(base_url, token)
```

---

# 65. 不要让 Embedded Runtime 依赖 config.toml

正式 Desktop：

```text
Embedded Runtime
```

使用程序生成配置。

`config.toml`：

```text
只服务 CLI / Developer
```

桌面用户不能因为：

```text
config.toml 不存在
```

导致启动失败。

---

# 66. 数据目录迁移

已有 V1：

```text
%LOCALAPPDATA%\AIKnowledgeSync\aiks.db
```

必须继续使用。

Desktop V2 不能：

```text
创建另一份 State DB
```

导致之前状态丢失。

最终：

```text
同一个 aiks.db
```

供：

```text
CLI
Desktop
```

使用。

---

# 67. 当前历史数据不能丢

修改 Runtime 不应影响：

```text
134 Codex
8 Gemini
518 OpenCode
```

Provider 扫描结果。

Runtime 修复完成后：

```text
aiks scan
```

仍应：

```text
约 660 Session
```

这是重要回归测试。

---

# 68. Release 构建最终流程

最终只需要：

```powershell
.\scripts\build-release.ps1
```

自动：

```text
Check Runtime
    ↓
Setup if missing
    ↓
Verify SiYuan
    ↓
cargo test
    ↓
frontend install/build
    ↓
Tauri build
    ↓
NSIS AIKS Installer
```

输出：

```text
target/release/bundle/nsis/
```

最终：

```text
AIKS-Setup-x64.exe
```

---

# 69. 最终用户流程

最终用户不得执行：

```text
setup-siyuan.ps1
dev.ps1
cargo
npm
powershell
```

这些全部只是：

```text
开发者工具
```

用户只需要：

```text
AIKS-Setup-x64.exe
↓
安装
↓
AIKS
```

---

# 70. V2 Runtime Fix 验收清单

只有以下全部通过才允许宣布修复完成：

```text
[ ] siyuan.version = 3.8.3

[ ] 不再存在 win.zip 假设

[ ] GitHub Asset 通过 Release metadata 精确解析

[ ] Asset SHA256 校验

[ ] setup 支持 Windows PowerShell 5.1

[ ] ProjectRoot 路径正确

[ ] Runtime 官方目录结构保留

[ ] Kernel 可执行

[ ] serve --help 正常

[ ] stage 存在

[ ] appearance 存在

[ ] guide 存在

[ ] Kernel 可启动

[ ] 只监听本机

[ ] /api/system/version 返回 3.8.3

[ ] 不需要 SIYUAN_TOKEN

[ ] 不需要 AccessAuthCode

[ ] SiyuanClient token 可为空

[ ] Desktop 自动启动 Kernel

[ ] Desktop 自动停止 Kernel

[ ] Desktop Crash 可恢复

[ ] Knowledge Window 可以直接打开

[ ] 不显示 SiYuan 登录页

[ ] AIKS Notebook 自动创建

[ ] 660 个现有 Session 仍可扫描

[ ] Session 可真正写入 Knowledge Base

[ ] File Watcher 可实时更新

[ ] Periodic Scanner 保留

[ ] dev.ps1 一条命令可启动

[ ] build-release.ps1 一条命令可构建

[ ] 最终安装包包含 Runtime

[ ] 干净 Windows 不安装 SiYuan即可运行

[ ] 干净 Windows 不设置任何 Token 即可运行
```

---

# 71. 最终人工验收流程

开发完成后严格执行：

## A. 清理开发状态

不要使用：

```text
已经手动安装的 SiYuan
```

必须模拟：

```text
机器从未安装 SiYuan
```

---

## B. 开发启动

只执行：

```powershell
.\scripts\dev.ps1
```

预期：

```text
自动准备/发现 Runtime
↓
启动 AIKS Desktop
↓
启动 Embedded SiYuan
↓
Control Center 正常
↓
Knowledge Window 正常
```

---

## C. Release

只执行：

```powershell
.\scripts\build-release.ps1
```

得到：

```text
AIKS-Setup-x64.exe
```

---

## D. 干净 Windows

安装：

```text
AIKS-Setup-x64.exe
```

然后：

```text
双击 AIKS
```

不得执行任何其他操作。

---

## E. 最终结果

应该看到：

```text
AIKS

知识引擎        Ready

Codex           134
Gemini            8
OpenCode        518
Claude            0

Total           660
```

知识库中可以搜索：

```text
ses_f717
```

并看到真实对话。

此时整个链路才算完成：

```text
OpenCode/Codex/Gemini
        ↓
AIKS Provider
        ↓
Canonical Model
        ↓
Sync Engine
        ↓
Embedded SiYuan
        ↓
AIKS Knowledge Desktop
```

---

# 72. 给 Coding Agent 的完整执行要求

将以下内容作为本次 Coding Agent 的任务：

> 对当前 AIKS V2 项目执行 Embedded SiYuan Runtime 全链路修复。
>
> 不允许只修复 setup-siyuan.ps1 中的下载 URL。
>
> 当前问题属于版本锁定、Release Asset、Runtime 准备、Kernel 启动、Bootstrap、SiYuan Client、Tauri Resource 和 Release Packaging 的整体集成问题。
>
> 首先阅读本实施文档。
>
> 然后检查当前代码，输出实际影响文件清单。
>
> 固定 SiYuan 3.8.3 Windows x64。
>
> 官方 Windows Asset 为 `siyuan-3.8.3-win.exe`，不得继续假设存在 Windows ZIP。
>
> 必须校验 SHA256：
>
> `49cd67c6e892aff5a04189707007df3046d48c0dc279840433b9824edd374569`
>
> setup-siyuan.ps1 必须支持 Windows PowerShell 5.1。
>
> SiYuan Runtime 最终保持官方资源结构：
>
> `runtime_root/kernel/SiYuan-Kernel.exe`
>
> `runtime_root/stage/`
>
> `runtime_root/appearance/`
>
> `runtime_root/guide/`
>
> Runtime 启动必须使用 SiYuan 3.8.3 的 `serve` 子命令。
>
> Embedded Mode 默认不配置 accessAuthCode，也不要求 SIYUAN_TOKEN。
>
> 必须保证使用 AIKS 专用 Workspace、NetworkServe=false、Kernel 仅监听 127.0.0.1。
>
> SiyuanClient 必须支持 token=None 的 Embedded Mode。
>
> 不允许最终用户首次打开 AIKS 时再从互联网下载 SiYuan。
>
> SiYuan Runtime 必须在 AIKS 构建阶段准备，并打包进入最终安装程序。
>
> 不允许重新实现 Provider、Canonical Model、Sync Engine、Renderer 等已经完成的模块。
>
> 每修改一个 Runtime 模块必须增加测试。
>
> 完成后执行：
>
> `cargo check --workspace`
>
> `cargo test --workspace`
>
> `aiks scan`
>
> Embedded Kernel Smoke Test
>
> Desktop Dev E2E
>
> Release Installer E2E
>
> 最后在没有安装 SiYuan、没有 SIYUAN_TOKEN 的干净 Windows 环境验证：
>
> `安装 AIKS → 双击 → 自动发现 Session → 自动导入 → Knowledge UI 可查看`
>
> 所有步骤通过后才能将本任务标记完成。

---

# 73. 修复完成后的正确开发体验

第一次 Clone 项目：

```powershell
.\scripts\setup-siyuan.ps1
```

以后开发：

```powershell
.\scripts\dev.ps1
```

发布：

```powershell
.\scripts\build-release.ps1
```

最终用户：

```text
什么脚本都不执行。
```

只使用：

```text
AIKS
```

---

# 74. 最终目标

修复之前：

```text
AIKS
+
脚本
+
外部 SiYuan
+
Token
+
端口
+
人工配置
```

修复之后：

```text
┌────────────────────────────────┐
│          AIKS Desktop          │
│                                │
│  Session Collector             │
│  Knowledge Sync                │
│  Embedded SiYuan Runtime       │
│  Knowledge UI                  │
│  Search                        │
│  Notes                         │
│  Auto Sync                     │
│                                │
└────────────────────────────────┘
```

用户只感知：

```text
AIKS
```

而不会感知：

```text
SiYuan
Kernel
HTTP API
Token
Port
JSONL
SQLite
WAL
Tauri Runtime
```

这才是 AIKS V2 的正式完成形态。