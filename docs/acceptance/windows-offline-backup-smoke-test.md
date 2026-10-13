# Windows AIKS 离线备份集中验收（独立测试目录）

**适用分支：** `feature/offline-knowledge-evolution-20261009`。本手册供 Windows 人工冒烟验收，不代表测试已经执行。

## 1. 只使用独立数据目录

在 **新的 PowerShell 会话**中运行以下命令（这些命令不删除已有数据）：

```powershell
cd E:\Project\Htrs\AISK
$env:AIKS_DATA_DIR = "E:\AIKS-Test\data"
Remove-Item Env:AIKS_BACKEND_MODE -ErrorAction SilentlyContinue
Remove-Item Env:AIKS_COLLECTOR_TOKEN -ErrorAction SilentlyContinue
Remove-Item Env:AIKS_WEKNORA_API_KEY -ErrorAction SilentlyContinue
Remove-Item Env:SIYUAN_URL -ErrorAction SilentlyContinue
powershell -ExecutionPolicy Bypass -File .\scripts\verify-offline-backup-readiness.ps1 -TestRoot $env:AIKS_DATA_DIR
.\scripts\dev.ps1
```

脚本只验证 git 分支、路径隔离、模式变量和所需源码文件；**不会创建、删除或覆盖**测试目录，不会启动内核，也不会测试备份成功。禁止将正式 AIKS 目录用于本测试。

## 2. 集中功能验收

- 首次启动：确认 SiYuan Ready，UI 允许创建测试文档，独立测试目录出现 SQLite 和 workspace。
- 受控备份：设置页输入数据目录外**全新**备份路径并重复确认；过程中不要使用外部程序写 SiYuan。确认完整备份、manifest、`sqlite/state.sqlite` 和 `siyuan/data/` 文档。
- 恢复：先执行完整性及磁盘容量预检；将 bundle 恢复至**另一个不存在的新目录**，对比测试文档；当前活动目录不得被覆盖或切换。
- 禁止场景：已存在的目标目录、活动数据目录内部目标、二次确认不一致、磁盘不足、外部 SiYuan 覆盖模式均应拒绝。
- 重启与故障：备份后 SiYuan 必须再次 Ready；若停机或重启失败，检查故障锁定状态，后续写入应被阻止。**不得强行解锁**；仅在真实健康状态恢复后执行界面上的核验。
- 数据保护：旧 SiYuan 文档、Provider 原始目录和个人数据均不得修改；检查 bundle 中不得存在 `siyuan/conf/` 或任何 Provider 凭据。

## 3. 失败记录

记录当前 commit SHA、Windows 版本、命令和显示的具体错误、备份输出目录、恢复目录，以及 `E:\AIKS-Test\data\logs\aiks.log` 的相关时间段。先移除日志中所有令牌及个人内容再分享。

**注意：** CI 与 Windows Provider 编译全部通过，也不能替代以上真实内核停机/重启和文件一致性验证。只有实际验收通过才可以声称 S1.3 真机完成。
