# Windows Provider 真实环境回归（只读）

AIKS S1.4 的真实安装环境验收不能由 GitHub 合成 fixtures 代替。本脚本专门在 **安装了 Provider 的 Windows 机器** 上重复发现会话，只采集耗时与退出状态。

## 执行方式

在仓库根目录使用 PowerShell 5.1 或 7：

```powershell
./scripts/windows-provider-baseline.ps1 -ConfigPath ./config.toml -Runs 3
```

首次运行会执行 `cargo build -p aiks-cli`。脚本临时设置一个全新 `AIKS_DATA_DIR`，不清空或覆盖现有 AIKS 数据；仅调用 `aiks-cli scan`，不会对 Claude/Codex/OpenCode/Copilot 的源目录写入。输出 JSON 报告只有每轮总耗时及退出码，不包含 Session 正文、Token、路径或配置内容。

如果需排查失败，请在具有权限的本机单独查看 CLI 输出，禁止直接把原始私密日志上传公共 Issue。此脚本不会以 0 结果推断所有来源兼容，也不会替代持续写入、WAL/SHM、Symlink/Junction、半写文件、真实大规模 Sync 的进一步验收。

## 验收口径

检查运行 3 次时首轮与后两轮耗时差异；对 Codex、Claude Code、OpenCode、Copilot 每个实际安装版本保留匿名化样本，并观察原 Session 身份不变、半写文件不会被认定完整、已有同步知识不丢失。手工检查 Provider 目录修改时间与权限边界，确认源数据只读。

实际显存/内存、CPU 与 I/O 需结合本机任务管理器或性能监视器采样，不能用 GitHub Actions 共享 Runner 的耗时冒充真实用户指标。

完整备份功能尚处于最后实施阶段；运行本脚本**不是备份，也不是恢复**。
