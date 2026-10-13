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


## 2026-10-12 Windows 实际磁盘容量预检与恢复时复核

- 桌面恢复新增 `inspect_offline_backup_capacity`，Windows 使用系统 `GetDiskFreeSpaceExW` 查询目标卷的调用方可用字节数，调用 Core 的容量预算验证（归档数据大小 + 至少 64 MiB 或 20% 预留）。
- 原有设置页面的恢复预览调用新命令展示实际容量与所需空间；用户最终确认执行时再次读取实际容量，不信任过期前端数据，也不接受前端随意提供的剩余空间数值。
- Windows API 不可用或磁盘容量不足时拒绝恢复；其他平台暂时显式拒绝 Desktop 恢复执行，而非假定有空间。原只读清单预检命令保留兼容。
- 不允许覆盖运行中的数据，尚未完成 SiYuan 正式停写协调、备份创建界面或最终真实 Windows 恢复验收；此提交须待新 CI 验证。


## 2026-10-12 Windows 容量门禁命令注册与 CI 修复

- 基线 `780ccd9`：Windows Provider #38217257362 成功；综合 CI #38217257315 因 backup_commands.rs 一处 Rust 格式差异阻断，未进行后续 Clippy/测试。
- 本批修正 Rust formatting，补充 `inspect_offline_backup_capacity` 的 Tauri generate_handler 注册，避免 Settings UI 调用时出现 unknown command；容量预检仍由 Windows 系统查询并在确认恢复前再次执行。
- 仍需完整 SiYuan 停写协调与 Desktop 创建备份。仅 CI 通过不能替代真实 Windows 验收。


## 2026-10-12 独立已停写工作空间创建备份入口

- 新增 `create_offline_backup_from_stopped_workspace` Tauri 命令及设置页创建操作，允许从用户事先停写的**独立** SiYuan 工作空间生成 SQLite 快照及文件归档，调用 Core 的前后 SHA-256 稳定性校验。
- 对来源目录及输出目录均严格执行与当前 AIKS 数据根目录的双向路径隔离，禁止从运行中的工作空间直接创建备份；输出与来源不得嵌套，必须是不存在的独立目录。
- 此手动入口不负责进程停写、不保证已停写来源与当前 SQLite 状态处于同一时间点；因此仍不属于完整 S1.3 在线应用一致性备份。后续需实现真正的生命周期编排并通过 Windows 真机验证。


## 2026-10-12 创建备份入口安全修复

- 基线 `0290219` Windows Provider #38218662638 通过，综合 CI #38218662654 因 backup_commands.rs 两处 rustfmt 差异失败；依据 CI 给出的精确差异统一修复。
- 手动备份入口增加当前 aiks.db 文件类型门禁：必须预先存在且为普通文件；缺失或符号链接时在调用 StateDb::open 前拒绝，避免打开数据库自动创建一个新的空 SQLite 并将其误当作现有用户数据备份。
- 该入口仍只支持用户自行停写的独立 SiYuan 工作空间，不代表完整停写协同或当前 AIKS+SiYuan 的原子一致性；对真机测试仍保持阻断。


## 2026-10-12 备份格式收敛与外部运行时安全门禁

- 综合 CI #38219225779 只在 Rust formatting 失败，Windows Provider #38219225781 成功。按照 CI 差异修正 StateDb::open 单行格式。
- 手动离线备份入口显式拒绝 SIYUAN_URL 等外部 SiYuan 开发覆盖模式，避免用户误以为 AIKS 能停写并控制外部进程。
- 这仍不构成真正的停写协调。后台同步、AI 任务、watcher、SiYuan 内核和一致性切点必须在后续完整闭环中一起处理。


## 2026-10-12 后台写入排空协调基础

- 新增 `storage::backup_quiescence::BackupQuiescence` 及 RAII 写者许可/停写守卫，准备为 watcher、周期同步、AI 作业以及用户命令共享统一门禁；停写前等待活跃写入排空，停写期间拒绝新增许可，守卫释放后恢复接收写入。
- 单元测试覆盖并发排空、新写入拒绝、重复暂停拒绝以及恢复；停写方法是阻塞式，实际调用时必须放到阻塞线程，不得阻塞 Tauri 异步执行器。
- **当前仅为协调原语，尚未连接所有真实业务写入点，也尚未自动停止 SiYuan。未经全路径接入，不可视为 S1.3 完整停写实现。**


## 2026-10-12 实际后台扫描任务接入写入许可

- 在 Tauri 应用根状态管理共享 `Arc<BackupQuiescence>`，并让启动初扫、Watcher 同步、周期扫描调用 `sync_and_enqueue_extraction_with_trigger` 时持有写者许可跨越整个异步执行阶段。暂停时拒绝新的这三类同步，等待已开始的任务释放许可。
- 当前覆盖的是三个具体后台扫描入口；内容迁移、异步 AI/Pipeline worker、Tauri 用户写命令、SiYuan 内核仍可能产生写入，必须继续接入。**现阶段严禁把该门禁当成可执行的一键停写备份。**


## 2026-10-12 用户主动同步与内容迁移实际接入

- 基线 `230dd21` Windows Provider #38221275162 成功；综合 CI #38221274992 因 lifecycle.rs import 排序一处 rustfmt 差异失败。精确修正该差异。
- 在 Tauri 用户主动触发的 sync_now、sync_and_extract、sync_knowledge_to_siyuan 命令持有全局 BackupQuiescence 写者许可，暂停时返回明确错误。
- 启动内容迁移在实际 migrate 调用前取得写者许可，避免它绕过后台同步的门禁。依旧存在 Pipeline Worker、其他直接变更命令、运行中 SiYuan 内核等未覆盖写入点；当前停写门禁不得用于批准应用级备份。


## 2026-10-12 用户 AI 任务入口写入排空接入

- 基线 `9e58bd4` CI #38221967218 因 Rust 格式差异失败；按差异修复 commands.rs 与 lifecycle.rs。
- 在 `extract_session_now`、`retry_failed_ai_task`、`cancel_pending_ai_task`、`run_pipeline_for_session`、`backfill_extractions` 的 Tauri 人工写入入口增加跨异步调用的写者许可。
- 仍有 Pipeline Worker 内部、知识编辑命令及 SiYuan 内核运行时尚未纳入完整停写；不可将当前功能当作运行中一键备份。


## 2026-10-12 知识与项目写命令接入停写协调

- 基线 `44877ad` 的综合 CI #38222700174 与 Windows Provider #38222700212 均成功。
- 本批把知识手动创建、整理衍生、收藏、归档、恢复、发布到 SiYuan、反馈、关系建议与关系审核，以及项目 Session 绑定/解绑、项目复盘创建、报告草稿保存等共 13 个 Tauri 写入入口纳入共享 BackupQuiescence 写者许可。操作未完成时许可不释放；备份暂停期间新操作拒绝进入。
- 这只是具体命令层的覆盖；Pipeline Worker 和其他异步/内部写入、SiYuan 内核停写以及完整停止/恢复流程仍须后续核验，不得开放当前活动空间的一键备份。


## 2026-10-12 业务门禁签名变更的内部调用修复

- 基线 `1bcbc4b` Windows Provider #38223597579 成功，但综合 CI #38223597444 的 Rust Clippy 和 Windows desktop compile 均报 E0061：`create_organized_knowledge` 调用 `create_knowledge` 时漏传第三个 BackupQuiescence State 参数。
- 修正内部转调时传递同一个状态门禁，使两个级别都获取写者许可并在异步写入返回后释放。此项修复不等于全部 Pipeline Worker 写入已经接入，不应据此开放运行中备份。


## 2026-10-12 源码契约与真实门禁一致性

- `3779d27` 的 Rust quality/Windows desktop compile/Windows Provider 均通过；综合 CI #38224345308 仅前端源码契约组织知识测试仍断言旧的 `create_knowledge(input, state)` 签名。修正为实际已编译的带 `gate.clone()` 内部调用。
- 新增独立 Vitest 门禁契约，静态核查此前覆盖的 21 个用户写入命令都具备写者许可，并核查嵌套知识创建传递相同的门禁。此覆盖为防回退检查，不替代 Pipeline Worker 和 SiYuan 内核的停写实现。


## 2026-10-12 Pipeline Worker 任务领取与异步处理门禁

- 已添加 Arc 自持有 writer permit，长任务跨 Tokio spawn 生命周期持有许可，直到真实 Job 退出后才释放。
- Worker 在 claim_next 之前尝试获得许可，暂停时不再领取新任务；已经运行的任务持有许可并参与 drain。Desktop 内嵌和外部启动路径将共享门禁注入 AiksEngine/Worker。
- 当前仍无全面的一键停写流程；后台 AI 之外的其他内核运行时、对外 SiYuan 并发编辑、UI 边界的原子一致性尚需端到端检查。该提交需完整 CI 验证，不能提前认可 Windows 备份测试。


## 2026-10-12 Worker 自持有许可实际回归

- `a08d9b6` Windows Provider #38226112922 成功，综合 CI #38226112921 只在 Rust formatting 失败；依据差异修正 worker.rs import 和 Self 初始化格式。
- 补充长作业 OwnedWriterPermit 的并发单测：任务许可仍持有时暂停等待，许可结束才完成排空、暂停期间禁止新写入，退出后恢复许可。
- **重要启动时序风险**：Worker 在 AiksEngine::initialize 时已启动，而 Desktop 仅在 initialize 返回后注入 backup gate。这段间隙仍可能领取任务，故此阶段不能启动活动工作区完整备份；后续必须使门禁在 Worker 开始轮询之前绑定，并完成 SiYuan 运行时真正停写协调。


## 2026-10-12 Worker 控制参数合并与 CI Clippy 收敛

- 基线 `2ed9a71`：Windows Provider #38230245064 成功，综合 CI #38230245218 已通过 Rust formatting，但 Clippy `too_many_arguments` 阻断于 run_worker 新增第八个参数。
- 将 Worker 的 Semaphore 与共享 BackupQuiescence 状态组合为一个控制参数，在入口解构，保留现有 Worker 领取任务前获取写许可和任务结束释放的行为，不使用 `#[allow]` 绕过质量门禁。
- 仍未解决 AiksEngine::initialize 内部启动 Worker 到 Desktop 注入门禁的时间窗口。应用级一键备份仍禁止启用，必须后续完成真正运行时一致性编排。


## 2026-10-12 Worker 启动前门禁绑定

- 基线 `f40c21c` 综合 CI #38230854196 与 Windows Provider #38230854269 成功。
- AiksEngine 新增可选初始 BackupQuiescence 注入构造入口，PipelineWorker 的启动参数在 spawn 之前携带已绑定的门禁，Desktop 所有三条 engine 初始化路径都在调用 initialize 时传入共享门禁，消除初始化完成后 set_backup_gate 带来的 Worker 无保护启动空窗。
- 原 CLI 与测试依然可调用 initialize/start_without_gate，避免意外改变非 Desktop 运行语义。尚未实现完整 SiYuan 停止和备份过程协调，不允许据此开放活动数据一键备份。


## 2026-10-12 Worker 初始化门禁时序防回退

- 基线 `146b68c` Windows Provider #38231925966 成功，综合 CI #38231926216 在生命周期初始化的三处 Rust 格式差异中断；依据 CI 逐处修正，尚待 Clippy 和 Windows compile 验证。
- 新增 Desktop Vitest 源码契约，断言 Worker 初始门禁在 Tokio spawn 前就存在、AiksEngine 在创建 Worker 时传递 gate、Desktop 三条初始化路径都使用初始化式注入而不是后置注入。
- 这项防回退只证明代码静态结构，不证明真实 SiYuan 停写、全部异步任务排空或完整备份恢复可靠。


## 2026-10-12 可跨异步任务持有的停写守卫

- 基线 `59c3d9a` 综合 CI #38233961014、Windows Provider #38233961121 均通过。
- 新增 `BackupQuiescence::pause_and_drain_owned`，同步阻塞至既有写者退出后返回 Arc 自持有的停写守卫；守卫生命周期不依赖局部引用，可在编排过程中跨异步阶段保存，释放时恢复接纳写入。明确调用必须置于阻塞线程。
- 补充独立线程并发回归：活跃长任务阻塞 drain、开始停写后拒绝新许可、退出后恢复正常。
- 下一阶段仍需使用该守卫实现 SiYuan stop → 校验已停止 → snapshot → start 的保证恢复流程。内部异步写入覆盖审计尚未完成，故未开放活动工作空间一键备份入口。


## 2026-10-12 停写超时自动恢复写入门禁

- 基线 `9f7e02f`：Windows Provider #38234676740 成功，综合 CI #38234676753 因 `pause_and_drain_owned` 格式差异失败。按 CI 精确修正。
- 新增 `pause_and_drain_owned_with_timeout`：在阻塞线程等待已有任务排空，超时后清除暂停状态并唤醒等待者，避免失败导致写入门禁永久关闭；成功时仍由 RAII 守卫控制备份期间写入禁令。包含任务未结束时的超时恢复及成功释放单测。
- 必须在实际生命周期编排中传入合适的超时。尚未把 SiYuan 退出、备份和启动流程连接到该门禁，也未完成所有写入路径审计，因此没有开放当前活动工作空间的一键备份。


## 2026-10-12 内部运行时停止/备份/重启编排（尚未开放）

- 基线 `23ec7c9`：Windows Provider #38235311140 成功，CI #38235311138 因停写超时回归测试一处 rustfmt 差异失败，现依日志修正。
- 新增内部 **未注册命令** `create_managed_quiesced_backup`：阻塞线程限时排空 → 获取受控 SiYuan 运行时 → 检查 Ready → stop → 确认 Stopped → 在阻塞线程完成 workspace+SQLite 归档 → 无论成功失败均尝试 start → 最后释放停写守卫；清楚区分备份与重启两种错误。
- 这是内部编排骨架，不是已经通过验收的可用一键备份。尚需验证全部写路径、失败处理、运行时监控交互、应用级数据一致性与 Windows 真机，才允许把命令注册到 UI。禁止绕过停写保护直接备份当前数据。


## 2026-10-12 Windows 分阶段真机测试的明确边界

- HEAD `d620dfa`：Windows Provider #38238751337 成功，综合 CI #38238751357 在 `backup_commands.rs` 单处 rustfmt 差异处失败；已按日志修正，后续 Rust Clippy/测试等待新 HEAD 验证。
- 可以在独立的 `E:\\AIKS-Test\\data` 目录启动个人离线模式，对**除活动工作空间完整备份恢复以外**的已实现功能做非破坏性的 Windows 冒烟测试；只将结果视为阶段性反馈，不是 S1.3 最终验收。
- 活动工作空间一键完整备份仍未注册到 Tauri/UI：内部 stop-snapshot-start coordinator 未通过综合 CI 且缺少外部写入审计、重启失败恢复保障。不得修改或覆盖现有用户生产工作区。
- 冒烟测试先验证隔离目录生效、首次启动、手动同步与知识检索、启动/退出重入、日志中无异常；真实 SiYuan 资产与备份恢复仅在最终专门验收阶段处理。


## 2026-10-12 受控备份来源与目标路径绑定

- 基线 `c7bcfec` 综合 CI #38239409242、Windows Provider #38239409192 均 success。
- 对内部 stop/snapshot/restart 编排新增前置路径验证：仅允许当前数据根目录下的 aiks.db 和 siyuan/workspace 作为来源，输出必须在数据根目录外且为不存在的新目录；防止未来 UI 接入时将不可信的任意路径用于归档。
- 桌面 Rust 路径测试验证正确路径允许、目标嵌套与错误来源拒绝。内部编排仍未注册对外命令，全部写入覆盖/真实 SiYuan 退出与异常恢复仍待专项验证。


## 2026-10-12 受控备份路径前置验证的质量收尾

- HEAD `4aceafd` Windows Provider #38240349490 success，CI #38240349805 在 Rust 格式检查因 backup_commands.rs 三处差异失败，未运行 Clippy/Rust tests。按 CI 全部差异修正，不以仅 Provider 成功宣称完整验证。
- 新增已有备份目标目录的无覆盖回归：校验失败后目录仍保留，不删除用户原有内容。完整停写、SQLite/SiYuan 一致性与桌面一键备份依旧有未闭合风险。


## 2026-10-12 受控停写子进程退出证明与工作空间敏感目录保护（代码级，待 CI）

- 备份专用 `stop_for_backup` 仅接受本进程持有且 PID 完全匹配的 SiYuan `Child`，使用 `try_wait` 确认退出；恢复的无 Child PID、PID 不一致或已退出的假 Ready 一律在 stop 之前拒绝。优雅退出超时后仅杀死已验证的 owned child，失败时保留 runtime.json 与句柄，禁止将可能运行的进程标成 Stopped。
- 未注册的内部停写编排使用此接口而非原来的无条件 `stop()`；备份线程异常也进入重启分支，停写证明失败不触碰任何归档文件。
- 用户文档与附件归档限制在 `workspace/data/`，`workspace/conf/` 中的敏感配置不进入归档；恢复前同样拒绝未被批准的 `siyuan/` 目录。此限制不代表 SQLite 和知识正文不存在敏感信息，更不能称作整工作区全部插件设置无损备份。
- 补无 Child/PID 不符/进程已退出、排除 conf、伪造目录预检拒绝等回归。仍未注册运行中一键完整备份，亦未完成全部写路径/真实 Windows 停写/磁盘容量/失败回滚验收。


## 2026-10-13 Rust 门禁与受控停机事实

- 在 HEAD 3bd05dd3 中综合 CI #38264977181 因 runtime/mod.rs 两处及 app_backup.rs 一处格式差异，跳过了 Clippy/tests。按照原始 CI 格式差异统一修复，不改动 SiYuan 严格确认子进程退出的语义。
- 手动和自动开发均不得把仅创建 Git commit 对象当作已推送，须验证分支 HEAD 实际变化。没有本地 rustfmt 工具，以上代码仍必须由实际 CI 校验。
- 内部 create_managed_quiesced_backup 仍未注册到 UI；重启失败释放停写状态等安全缺口依然阻断活动工作区一键完整备份。


## 2026-10-13 归档白名单与可移植恢复测试对齐

- 基线 `f9579fc` 综合 CI #38331840586 的 Rust fmt/Clippy 和 329 项库单元测试均通过；集成测试 `backup_portable_restore::offline_restore_preview_checks_schema_paths_and_requires_detached_destination` 因旧测试使用 siyuan/notes/note.sy，未符合现行仅允许 siyuan/data/ 的安全白名单而失败。Windows Provider #38331840542 通过。
- 本次仅调整集成测试样例为 `data/notes/note.sy` 和对应独立恢复断言，绝不为让测试通过而放宽现行归档路径白名单。再次验证需要新 HEAD CI；Windows 真机和 SiYuan 受控备份尚未验收。


## 2026-10-13 SiYuan 重启失败后写入保护

- 基线 HEAD `52cebe2` CI #38332501937 / Windows Provider #38332501912 均成功。
- `BackupQuiescence` 新增需验证恢复的持久内存闩锁：重启失败时不再因排空守卫析构而重新接纳写入；只有后续明确验证 SiYuan 运行正常后才允许通过 `clear_after_verified_recovery` 解锁。
- 内部未注册的一键备份编排在 SiYuan 启动失败时调用该保护。补 Rust 测试验证失败封禁、二次暂停拒绝、明确解除后恢复。**尚无用户可操作的验证恢复 UI，不得发布活动数据备份功能。**


## 2026-10-13 受控故障恢复核验入口

- 基线 `c19f2e2` 的 CI #38333244987 与 Windows Provider #38333244996 success。
- 新增只针对由 AIKS 管理的已运行内嵌 SiYuan 的 `verify_managed_siyuan_recovery` Tauri 命令。只有故障封禁存在、运行时与健康检查同时确认 Ready，才执行解除写入门禁；外部 SiYuan、不存在运行时、健康异常和未触发故障封禁均拒绝。
- 添加门禁状态查询及单元测试，不提供不经核验的强制解锁。本入口尚未接入完整用户恢复向导，亦不能替代受控 SiYuan 重启尝试或最终故障注入验收。


## 2026-10-13 只读恢复状态与故障核验 UI

- 基线 `0d89433` Windows Provider #38333941416 success，综合 CI #38333941378 仅在 backup_commands.rs Rust formatting 失败。已按日志修正。
- 新增只读 `get_managed_backup_recovery_required` Tauri 命令并注册到 Handler；设置页新增手动查询锁定状态及条件触发的健康核验入口。只有明确查询出存在锁定时才展示核验操作；失败保持锁定、禁止强制解锁、不自动重启内核。
- 真正的受控一键备份仍未开放；故障恢复入口可交互不代表操作系统进程健康、数据一致性与异常重启的 Windows 真机测试已通过。


## 2026-10-13 故障恢复 UI 契约与 CI 修复

- 基线 `539edca` Windows Provider #38334450621 success，综合 CI #38334450615 仅在 backup_commands.rs 一处 Rust 函数签名格式差异失败。严格按照 rustfmt diff 修正。
- 新增 Desktop Vitest 恢复契约测试：只读状态、受控核验命令均注册，核验检查 SiYuan Ready，UI 只有明确存在故障锁定才显示核验动作，Mock 不执行真实恢复。此测试不取代真实内核故障注入。
- 运行中工作区一键完整备份仍暂未开放，不能从本次静态契约推出其已可真机验收。
