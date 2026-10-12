# AIKS S1.3 跨平台备份恢复预检：开发验收入口

本清单记录离线隔离恢复安全门禁，不是 Windows 真机或真实模型验收报告，也不表示 Desktop 备份入口已经可用。

## 自动化门禁

从仓库根目录运行：

- cargo fmt --all --check
- cargo clippy --workspace --all-targets -- -D warnings
- cargo test -p aiks-core --test backup_portable_restore
- cargo test -p aiks-core storage::backup

还需等待 CI 的 Rust 测试、Windows desktop 编译和 Windows Provider 任务全部通过。固定临时目录及 SQLite 样本不能替代真实 SiYuan 工作空间。

## 应用级安全契约

1. 先明确停止 SiYuan 写入。归档目的地必须在工作空间外，Provider 文件始终只读。
2. 应用归档清单只能包含 sqlite/state.sqlite、sqlite/state-manifest.json 与 siyuan/ 目录下的文件；SQLite 快照必须通过 SHA256、完整性、Schema 和外键检查。
3. 清单必须是规范化正斜杠路径。跨 Linux、macOS、Windows 时，CON、NUL.txt、COM1、ADS、末尾空格/点、大小写别名、文件/目录重叠、反斜线歧义都必须拒绝。
4. preflight_offline_restore 返回文件数、SiYuan 文件数和总归档数据字节数，只用于用户选择前的检查与提示。恢复位置必须在归档外且尚不存在。此接口不检查磁盘剩余空间。
5. restore_offline_backup_to_new_directory 再验证来源，仅复制到全新隔离目录，并检查恢复后的 SQLite；绝不自动覆盖正在运行的 SQLite、SiYuan 或原 Provider 文件。

## 后续 Windows 集中验收

- 关闭内嵌 SiYuan 并确认停写，备份真实含大量 Session、分卷及附件的工作空间；测量容量与执行耗时，检查配置秘密不会被打包。
- 在另一台 Windows 以及 Linux/macOS 环境通过预检，将快照恢复至独立目录，校验 Session、Knowledge、分卷、附件、全文与向量索引数及检索行为。
- 故障注入覆盖符号链接、损坏清单、大小写碰撞、磁盘不足、强退中断和旧 Schema；核对失败时用户原文与现有工作区不变。
- 独立实现 Desktop 用户确认、系统实际剩余空间检查与 SiYuan 停写/重启编排；这些部分尚未完成，不能向用户宣称 S1.3 已完整可用。


## 2026-10-12 Desktop 只读备份预检命令

- 已增加 `inspect_offline_backup` 和 `verify_offline_backup_manifest` 两个 Tauri 命令：根据用户明确提供的绝对路径，验证备份清单及独立恢复的目标目录合法性，返回文件数量/字节统计以及 SiYuan 必须停写的提示。
- 命令对真实数据根目录进行路径隔离，拒绝把目标放入当前 AIKS 数据目录或其上级；没有写入恢复目标，也没有自动停止、重启 SiYuan。加入相对路径拒绝测试。
- 目前只有只读 Core/Tauri API，还没有完整交互界面或用户确认后的安全执行链；不可据此宣称可用的桌面恢复已经实现。需要 Windows 实际数据目录及工作区验收。


## 2026-10-12 仅限独立目录的确认恢复命令

- 增加 Desktop Tauri `restore_offline_backup_isolated`：必须由前端传入与执行路径完全一致的二次确认目标，重跑只读预检与当前 AIKS 数据根目录隔离门禁，在阻塞线程中恢复，返回归档文件数。
- 仅创建全新隔离目录，不允许覆盖运行中的 SQLite、SiYuan 原文、Provider 文件或已有恢复目录；不自动替换当前数据目录、重启应用或迁移配置。
- 此命令尚未接入可交互 UI，也未验证系统实时可用磁盘空间；请勿将其当作最终可用的恢复功能。大数据量恢复、停写协调和恢复失败回滚仍需完整验收。


## 2026-10-12 Desktop 安全隔离恢复交互

- 设置页新增“离线备份 · 安全隔离恢复”：输入已有备份绝对路径和不存在的目标目录，调用真实 Tauri 预检显示文件数、SiYuan 文件数与字节数。预检失败不得继续执行。
- 用户必须再次准确输入已预检的目标目录；路径变更会撤销预检与确认，执行期间阻止重复提交。仅调用已有 `restore_offline_backup_isolated`，不会切换当前数据目录或覆盖运行中的 SiYuan。
- 新增纯逻辑 Vitest，覆盖目标一致、大小写差异、过期预检、空路径和并发阻断。浏览器 Mock 模式不伪造真实备份成功结果。
- 仍无真正的 SiYuan 停写协调、自动化可用容量检测和 Desktop 完整备份创建；当前界面只完成隔离恢复的人工确认链。CI 成功也不等于 Windows 真机验收。
