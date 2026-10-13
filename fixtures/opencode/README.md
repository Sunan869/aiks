# OpenCode Fixtures

本目录存放匿名化的 OpenCode Session 测试数据。

## Files

| File | Description |
|------|-------------|
| `session-fixture-1.db` | 匿名 SQLite fixture: 2 轮对话，含 bash tool call |

## Schema Compatibility

Fixtures use the schema verified from live OpenCode DB (2026-09-11):
- `session`: id, project_id, directory, title, version, time_created, time_updated, model
- `message`: id, session_id, time_created, time_updated, data (JSON)
- `part`: id, message_id, session_id, time_created, time_updated, data (JSON)
- `project`: id, worktree, name, time_created, time_updated, sandboxes

## Rules

- 禁止提交真实 API Key、Token、个人路径、公司机密。
- 每个新格式至少保留一个最小 fixture。
- fixture 必须对应 parser golden test。
- 上游格式变化时新增 fixture，不要覆盖旧 fixture。
