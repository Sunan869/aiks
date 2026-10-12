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
