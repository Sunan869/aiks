# AIKS Service S1：本地独立服务与桌面入口

本文件描述 `feature/aiks-service-extraction` 的 S1 使用范围。S1 是独立服务和本地桌面首条 HTTP 业务链路，不是已经交付团队权限、完整思源编辑代理、RAG 或新安装包。S2–S4 的需求保留；不要将它们标成 S1 已实现。

## Windows 开发启动

在仓库根目录执行：

```powershell
.\scripts\dev.ps1
```

脚本校验并准备锁定版本的思源运行资源，缺少 node_modules 时运行 npm ci，编译 `aiks-service`，再以显式 `service_local` 开发模式启动 Tauri/Vite。原生控制器随后启动本机思源和 AIKS Service，完成私有 stdin 握手与认证连接。前端不持有 Service 启动令牌、思源令牌或任意程序路径。

Service 构建失败不会继续启动旧二进制。退出脚本后恢复原进程环境变量与工作目录。不需要另开一个终端手动运行 Service；不要同时执行两套桌面启动命令。

运行原有个人版：

```powershell
.\scripts\dev.ps1 -Legacy
```

配置文件 `backend.mode` 默认仍为 `legacy`；仅接受 `legacy` 或 `service_local`。`dev.ps1` 显式选择新模式，`-Legacy` 选择旧模式。不通过这个脚本直接 `npm run tauri dev` 时，遵循已有配置。浏览器直接打开 Vite 页面不会连接私人数据或自动填充模拟结果；开发 mock 需要明确设置 `VITE_AIKS_MOCK=true`。

首次构建、npm 安装、运行资源与模型下载需要联网；已经准备好这些资源后，离线运行不需要公司服务器。新服务的模型必须显式启用且提供 endpoint/model；未配置模型时，采集、回执、会话阅读与关键词搜索仍可使用。

## 数据与配置位置

复用原有 AIKS 数据根的选择方式，但本次新模式使用独立子目录：

```text
<AIKS 数据根>/
  aiks.db                       原有个人库，不自动迁移
  siyuan/workspace/              原有思源工作区
  service-local/
    desktop-owner.aiks-lock     本机桌面启动所有权
    instance-id                 持久实例身份，不是令牌
    collector.db                客户端采集与重传队列
    data/aiks.db                Service 业务状态、快照、任务和索引
    config/models.toml          用户编辑的模型配置
    config/runtime.toml         原生控制器生成的运行配置，不手工改地址
    siyuan/workspace/            独立思源内容工作区
    logs/                       此模式的运行资源日志
```

托盘“打开数据目录”在新模式下打开 `service-local`，因此该目录内模型文件就是 `config/models.toml`。来源目录与开关继续从原有 `config/aiks.toml` 读取；不会把全部旧会话自动上传到新空间。可以先使用 `-Legacy` 在原有设置页面修改 Provider 目录，完全退出旧程序后再启动新模式。暂停/关闭来源不会删除服务已接收的内容。

`models.toml` 初次生成时 AI 和 Embedding 都关闭。将 endpoint、model、必要的 API Key 填写为实际配置后再启用对应能力；修改后完全退出并重启。本机模型示例：

```toml
[ai]
enabled = true
base_url = "http://127.0.0.1:11434/v1"
model = "填写本机已安装的对话模型名称"
api_key = ""

[embedding]
enabled = false
base_url = "http://127.0.0.1:11434/v1"
model = ""
```

模型名称示例是占位符，不能原样运行。完全离线时所有被启用的模型 endpoint 必须在本机；配置云端/公司模型就会向该 endpoint 发送相应输入，不能称为数据绝不出本机。脱敏为规则性辅助，不保证覆盖所有敏感内容。

## 可验收流程

1. 启动后显示“AIKS · 本地知识服务”，服务状态为已连接。不会同时初始化旧 AiksEngine/Worker。
2. 勾选来源，按需填写排除会话 ID，点击采集。每来源每批最多 100 条，再次采集继续下一批，遍历结束后下一轮重新检查变更。排除、残缺日志与超限有明确计数，不截断后假称完整。
3. 采集到的完整标准化内容先写客户端 outbox，再发送至绑定实例/空间。服务不可用时保留已登记来源队列；首次登记新来源需要对应实例可访问。不存在收到响应前就推进已接收版本的行为。
4. 上传区分别显示待发送、发送中、已接收、暂停/错误；点回执核对服务接收状态，点处理任务查看 PENDING/RUNNING/DONE/FAILED/SUPERSEDED。已接收不等于已提炼。未启用 AI 的 DONE 可以只有会话索引，不会捏造知识条目。
5. 搜索已接收会话或已产生知识，点击结果读取。正文中普通文本按文本呈现，不执行原始 HTML/脚本。没有结果和服务请求失败分别显示。列表当前显示最近 30 条，搜索最多返回 30 条，上传区显示最近 100 条。
6. 关闭窗口入托盘时继续保留 Service；托盘“退出”或配置为完全退出时，停止采集、等待自有 Service 处理退出并停止自有思源。重新启动后继续使用相同业务实例与未完成队列。

建议使用托盘“退出”正常结束后再关闭开发终端。强制杀进程/系统断电不等于正常退出：Service 的持久任务/队列可以恢复，已有思源 runtime 由原运行时恢复逻辑处理，不能承诺一次强制中止就没有任何残留进程。

## 旧资料接管与回滚

旧库接管已实现为 Core 的显式内部操作并有迁移测试，但本界面不会自动执行，也不会替用户创建备份。S1 默认先在独立空间验证，旧数据库与原思源资料保留。不要使用 reset-data，也不要直接复制一个仍在使用的 SQLite 主文件来“迁移”（可能漏 WAL）。

回到旧界面只需先完全退出新程序，再运行 `dev.ps1 -Legacy`。这不是回滚已经执行的业务库 migration，而是重新使用未被自动迁移的旧空间。已执行显式接管的用户须使用事先验证的完整备份恢复，不能用旧版本程序直接打开新版 schema。Desktop 与业务 CLI 获取同一所有权锁，冲突时拒绝并行写同一业务数据库。

## 安全与功能边界

S1 Service 只允许 numeric loopback personal 模式；拒绝 team/非回环监听。每请求校验认证、实例、Host/Origin，查询先按可信主体/空间过滤再截断候选。接收快照完整持久化后，服务任务不再依赖员工电脑上的路径。

没有通用 `/proxy/*`、SQL、任意文件/URL、进程启动或 HTTP shutdown 接口。新页面不嵌入全权限思源页面，不暴露底层内容服务地址与令牌。已发布正文必须通过固定内部适配器读取思源规范正文；思源不可用返回 `content_unavailable`，不伪装成本地缓存原文。

冲突上传保持绑定原实例、空间、payload、submission ID、expected_revision，不自动重定向或改版本覆盖新内容。准确的同请求重试由服务幂等回执恢复；真正的版本冲突保持 blocked 并显示原因，手工内容合并/跨空间同步不在 S1 中。

本阶段已有 AI Assist API 返回不落盘建议，但未交付通用聊天/RAG界面。普通 HTTP 文档写入、共享发布、项目/部门成员与权限、远程多用户监听、完整安装包在后续阶段。测试内部调用现有 publisher 不代表已经有可用的 HTTP 发布按钮。

## 验证依据

使用实际 Core、HTTP Service 二进制、SQLite、Provider 和原生桌面客户端模块；模型和思源上游是临时回环 fixture。网络隔离测试在单独 Linux network namespace 内只启动 loopback，并验证无外部路由后运行完整有模型的处理/发布/索引/读取测试。没有修改宿主机防火墙、扫描私有会话或调用真实模型账户。

候选 `095bc89` 已通过 Linux 全工作区测试/Clippy、Windows 原生客户端测试、前端测试与生产构建，以及网络隔离测试；格式修正与收口后的提交必须以该提交自己的 Actions 为准。新增 Windows PowerShell 5.1 脚本契约覆盖一键启动顺序、Legacy、构建失败中止与环境恢复。GUI 原生窗口视觉表现、真实模型质量、真实思源版本与用户大库性能仍需真机验证。

作者完成自审，没有独立 reviewer；CI 的思源占位资源不是可用安装包。既有 npm install 审计输出包含依赖漏洞告警，未将本次功能测试包装成安全审计通过，勿向不可信网络开放开发服务器；依赖升级和发布安全审查另行跟踪。
