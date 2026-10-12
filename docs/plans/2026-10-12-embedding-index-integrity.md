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
