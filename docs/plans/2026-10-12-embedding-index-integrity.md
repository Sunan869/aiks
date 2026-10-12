# AIKS 离线版：2026-10-12 Embedding 索引与检索完整性修复

关联总体路线图：`docs/plans/2026-10-09-offline-knowledge-evolution-roadmap.md`，阶段 S2.4 / S1.4。

## 本轮 CI 基线

- 分支原 HEAD：`ab212cc8ffa7cff668105a3c32ef6df750f0746e`。
- Windows Provider #38177883457：通过。
- 综合 CI #38177883519：Rust 格式检查两处失败，Clippy / Rust tests 未执行；Windows 编译、前端与仓库卫生通过。
- 本轮集中修复格式问题，并补充实质性 Embedding 生命周期完整性防线，统一一次多文件提交。

## 变更与回归

- V4.2 知识/Session 原生索引对任意 `EmbeddingProvider` 返回的向量二次校验：维度上限、维度一致、NaN/Inf、零范数，异常数据不写入 SQLite。
- 知识索引异常转为 `failed` 且允许重建；Session 索引异常继续保留可检索 FTS 文本和错误状态，下次成功向量返回可恢复。
- 混合检索查询仅接受恰好一条有效查询向量，范数必须为有限正数；损坏的历史向量（非 4 字节对齐、非有限值、零向量、错误维数）不再污染其它语义候选，全文检索继续可用。
- 新增合成 Provider 无效输出、失败后恢复、历史向量检索过滤回归测试；本记录不是模型质量或 Windows 真机验收结果。

## 边界

- 原始 Provider 只读，SiYuan 用户编辑原文不自动覆盖。
- 不新增 SQLite migration，不合并 main，不发布 Release，保留单机离线模式。
- 本轮提交后必须以 GitHub Actions 最终状态确认格式/Clippy/Rust 测试结果；完整备份 S1.3 仍在六个优化项和五个功能代码级完备后最后进行。

## Session 索引边界补齐（2026-10-12）

- 当前 CI #38193447949：Clippy 与 rustfmt 通过，Rust 库测试 277 通过、1 失败；失败用例证明 Session 路径未调用共享向量完整性校验。
- Session 分块 Embedding 与 Knowledge 统一使用 `validate_embedding_vector`：拒绝 NaN/Inf、零范数、维度超限和维度不一致，同时保持原始 FTS 文本可检索，成功重试能够补齐向量。
- 新增 Session 索引的 Inf、零范数、维度不一致、超过 16384 维回归；任何非法模型输出不得入库，不声称真实模型或 Windows 真机已验证。
- 此批不改 Provider、SiYuan 原文、迁移和完整备份入口。

## 多批次向量异常原子性回归

- CI #38194451869 因一处 rustfmt 差异中断，Windows Provider #38194451871 成功；该轮未执行 Rust 测试。
- 将格式修复与多批次索引回归合并：首批有效向量、后续批次返回 NaN 时整次向量写入保持为零，全文索引仍可检索；随后使用有效模型重试可恢复向量。
- 仍属合成模型测试，不能等同真实模型或 Windows 本机验收。

## 失败后完整恢复与数据状态校验（2026-10-12）

- 基于 #38194807202 日志，将导致 Rust 测试未执行的 `query_row` 格式阻塞与恢复状态回归一起处理。
- 多批次 Embedding 出错后，不仅断言 FTS 仍可检索、失败不留下半批向量，还验证再次构建成功后持久化向量数与 chunk 数一致，且 `session_index_state.last_error` 清除、状态为 `ready`。
- 测试为模拟 Provider 的确定性回归，尚需完整 CI 和真实 Windows/模型端点验收。

## 2026-10-12 Rust Clippy await 锁边界修复

- HEAD `34b805d` 的 CI #38195391358 在 Clippy `await_holding_lock` 失败；前端、Windows Desktop、卫生检查及 Windows Provider #38195391250 通过，但 Rust tests 未运行。
- 多批次异常恢复回归在验证 SQLite 状态时曾持有连接锁直至下一次异步索引调用，即使显式 `drop(conn)`，Clippy 仍追踪词法作用域。现在显式缩短 SQLite 查询 guard 的词法作用域，确保异步重试前已释放锁；这是异步索引安全边界修复，保持生产索引与数据不变。
- CI 未运行之前不声称此回归已通过，也不冒充真实模型或 Windows 实机验证。

## 2026-10-12 Session 同内容并发重建隔离

- HEAD `8728272` 的综合 CI #38195701363 与 Windows Provider #38195701393 均完成且通过。随后检查发现同一 Session 两轮相同正文 hash 的索引在异步模型返回时间错序时，旧请求可能通过仅检查 hash 的结束门禁，覆盖较新一代的索引向量。
- 结束阶段现在在 `BEGIN IMMEDIATE` 事务内部核对 status/hash 和当前持久 chunk IDs 与本轮生成的 IDs；不匹配即回滚拒绝，不允许旧代次模型输出写入新代次 chunk。增加模拟同内容重建交错、过期代次被拒、当前代次成功持久化的 SQLite 单测。
- 不引入 schema migration，不修改 Provider 文件或 SiYuan 用户原文；该测试不替代真实模型和 Windows 并发故障注入。

## 2026-10-12 并发代次保护构建修复与持久化断言

- `38c03aa` 的 CI #38196207401 与 Windows Provider #38196207404 均失败：`session.rs:378` 的 rusqlite `MappedRows` 临时值延长借用导致 E0597 编译错误；Rust 格式检查亦有一处 `query_row` 差异，相关测试尚未执行。
- 显式绑定 `query_map` 返回的 rows 再 collect，确保迭代器在 stmt 析构前释放；按 rustfmt 日志修正断言格式，保持事务级代次验证。
- 增强旧代次拒绝后的持久化断言：数据库仍记录新代次 chunk ID、未写入任何提前完成的向量，然后才允许新代次完成。该回归仍需新 CI 与真实 Windows/模型环境验证。

## 2026-10-12 已标记 ready 的索引持久化完整性自愈

- 基线 HEAD `2a69500` 综合 CI #38196527439 与 Windows Provider #38196527459 均成功。
- 审核发现如果 `session_index_state` 标记 ready、hash 与模型相符，但某条向量记录已经缺失或 chunk 数量异常，旧 `can_skip` 仅依赖状态元数据，会一直跳过重新索引、使语义检索永久漏召回。
- 本批新增跳过前的实际 SQLite 行数、向量数量、维度和向量字节长度完整性检查；检测到缺失或不匹配时重建而不是报告 skipped。若无 embedding，仅需确认 chunk 数与向量记录一致。
- 新增完整索引 → 幂等跳过 → 删除持久向量 → 自动重建 → 再次幂等跳过的真实 SQLite 回归。此守卫不代替对历史向量数值内容的检验，也不代表 Windows/真实模型验证。

## 2026-10-12 持久向量损坏后的安全重建回归

- 上一 HEAD `d0b070f` 的 Windows Provider #38196955693 成功；综合 CI #38196955673 只因 `session.rs` 新守卫的 rustfmt 差异失败，Clippy/Rust tests 未运行。本次依据 CI 日志一并修复全部格式差异。
- 新增实际 SQLite 非法向量持久化回归：人工损坏 blob 为非 4 字节对齐或 1 维数据，随后再索引同内容时不得误跳过，必须重新生成合法向量；恢复后再次扫描能幂等跳过。原有向量缺失重建、并发代次保护保持。
- 所有测试为合成模型、真实 SQLite 的代码级门禁，不声称 Windows 真机或真实 Embedding 模型通过。S1.3 最后实现。

## 2026-10-12 持久向量恢复精确核验

- HEAD `78ea107` 的 Windows Provider #38197500427 成功，综合 CI #38197500462 仅在 `session.rs:971` 单条断言的 rustfmt 失败，Rust Clippy/测试未执行；本批修复这一格式差异。
- 回归进一步校验损坏的持久向量恢复后不仅返回 embedded_count=1，还实际写入维数 2 和 8 字节的 f32 向量，随后相同 Session 再次索引须幂等跳过；不将状态记录上的 ready 当成向量内容完整性的证据。
- 固定模拟与 SQLite 回归尚需新 CI，真实模型与 Windows 实机仍待集中验收。

## 2026-10-12 持久模型身份一致性修复

- 基线 `56d539d`：综合 CI #38197850265、Windows Provider #38197850318 均成功。后续代码级审核发现 `session_index_state.embedding_model` 与单条 `session_embedding_record.model` 不一致时，旧跳过门禁只检查数量/维度/长度，可能把旧模型结果当作当前语义索引。
- 跳过前的 SQLite 检查新增每个向量记录的模型身份匹配；旧模型记录触发重新提取 Embedding，不误报 skipped，仍保持原 FTS、Provider 只读、SiYuan 原文不覆盖。
- 新增真实 SQLite 正常幂等、污染模型字段、重新向量化、持久模型恢复、再次幂等回归；不新增 migration。固定模型不代表 Windows/真实模型质量验收。S1.3 完整备份恢复继续最后实施。

## 2026-10-12 CI Rust 格式门禁收尾

- CI #38198376267 Rust quality 在 `session.rs` 两处格式检查失败；Windows Provider #38198376257、Windows Desktop 编译、前端及卫生检查均通过，Rust Clippy/测试未执行。
- 本次根据 runner 输出修正 `params!` 和 `query_row` 格式差异；保留模型身份校验业务逻辑，后续改为按完整功能模块集中审查、提交，优先非 Embedding 功能与最后的 S1.3 备份恢复。
