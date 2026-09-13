# V1 验收标准

## 1. Provider

- [ ] Claude Code 可以读取真实本地 Session。
- [ ] Codex 可以读取真实本地 Session。
- [ ] Gemini CLI 可以读取真实本地 Session。
- [ ] OpenCode 可以读取真实本地 SQLite Session。
- [ ] 四者全部转换到相同 Canonical Model。
- [ ] 单条损坏 Session 不影响同 Provider 其他 Session。
- [ ] 一个 Provider 失败不影响其他 Provider。

## 2. 第一次导入

场景：本机已经存在历史 Session。

要求：

- [ ] `aiks scan` 可发现 Session 数量。
- [ ] `aiks sync --dry-run` 正确显示 NEW 数量。
- [ ] `aiks sync` 正确建立 SiYuan 文档。
- [ ] 每个 Session 只创建一个文档。

## 3. Session 继续

场景：Codex/Claude/OpenCode/Gemini 同一个 Session 增加后续消息。

要求：

- [ ] 识别为 UPDATED。
- [ ] 更新原 SiYuan 文档。
- [ ] 不创建第二个 Session 文档。

## 4. 重启

要求：

- [ ] 重启 AIKS 后历史 Session 不重复导入。
- [ ] 未变化 Session 标记 UNCHANGED。

## 5. OpenCode WAL

场景：OpenCode 正在运行并持续写 SQLite/WAL。

要求：

- [ ] AIKS 可读取。
- [ ] AIKS 不锁死 OpenCode。
- [ ] AIKS 不写数据库。

## 6. SiYuan Offline

要求：

- [ ] SiYuan 不在线时 Session 仍可扫描。
- [ ] sync_target 状态进入 PENDING/FAILED_RETRYABLE。
- [ ] SiYuan 恢复后自动补同步。

## 7. Conflict

场景：用户手工修改 AIKS 托管 SiYuan 文档后，上游 Session 又有更新。

要求：

- [ ] 检测 target hash 变化。
- [ ] 状态进入 CONFLICT。
- [ ] 默认不覆盖。
- [ ] `--overwrite` 可显式覆盖。

## 8. Missing Source

场景：原始 Session 被用户删除。

要求：

- [ ] SiYuan 文档保留。
- [ ] source_session.is_missing=true。
- [ ] 不自动删知识库数据。

## 9. Security

- [ ] 常见 Bearer Token 脱敏。
- [ ] API Key 脱敏。
- [ ] password/token/secret 脱敏。
- [ ] 日志不打印完整敏感信息。

## 10. Renderer

- [ ] User/Assistant 文本正确。
- [ ] Tool Call 正确。
- [ ] Tool Result 长内容截断。
- [ ] Thinking 默认排除。
- [ ] 附件缺失不导致同步失败。

## 11. SiYuan Metadata

每个托管文档至少包含：

- [ ] custom-aiks-managed
- [ ] custom-aiks-source
- [ ] custom-aiks-session-id
- [ ] custom-aiks-content-hash
- [ ] custom-aiks-parser-version
- [ ] custom-aiks-synced-at

## 12. CLI

以下命令均可运行：

- [ ] aiks doctor
- [ ] aiks scan
- [ ] aiks sync
- [ ] aiks sync --source codex
- [ ] aiks sync --dry-run
- [ ] aiks status
- [ ] aiks daemon
- [ ] aiks resync
- [ ] aiks rebuild-state

## 13. Tests

- [ ] Claude fixture/golden tests。
- [ ] Codex fixture/golden tests。
- [ ] Gemini fixture/golden tests。
- [ ] OpenCode fixture/golden tests。
- [ ] Markdown renderer tests。
- [ ] Secret sanitizer tests。
- [ ] Sync state tests。
- [ ] SiYuan integration tests。

## 14. Release

- [ ] Windows 可以直接运行。
- [ ] README 完整。
- [ ] config.example.toml 完整。
- [ ] THIRD_PARTY_NOTICES.md 已固定 Commit SHA。
- [ ] 无未说明的第三方复制代码。
