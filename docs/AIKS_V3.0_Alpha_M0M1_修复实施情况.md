# AIKS V3.0 Alpha 稳定性修复实施情况

日期：2026-09-13  
基线：49ae827（Phase A-E）  
本轮修复：commit 基于审计报告实施  
验收：107 个测试全部通过，EXE 打包成功（70 MB）

---

## 一、验收结果

```
✅ cargo test --workspace
   - 92 个 lib 单元测试通过
   - 15 个审计复现测试通过（全部期望行为验证）
   - 0 失败

✅ scripts\build-release.ps1
   - AIKS_0.2.0_x64-setup.exe (70 MB)
```

---

## 二、本轮修复明细（按优先级）

### M0：稳定性止损

#### B01 ✅ StateDb unsafe Sync 已修复
**问题**：`Connection` 被强制声明 `unsafe impl Sync`，实际上 `rusqlite::Connection` 不是 Sync，并发调用存在数据竞争。

**修复**：将 `Connection` 包装在 `Mutex<Connection>` 中：
```rust
pub struct StateDb { conn: Mutex<Connection> }
```
所有调用方通过 `db.conn()` 获取 `MutexGuard`，在作用域结束时自动释放。规则：不得在持有锁时 await HTTP/AI 调用。

**测试**：b12_worker_marks_failed_on_provider_error（Worker + DB 并发正常完成）

---

#### B05 ✅ Unicode/CJK panic 已修复
**问题**：多处使用 `&text[..N]`，中文字符在 UTF-8 边界截断时 panic。

**修复**：新增 `util/text.rs`：
- `truncate_chars(s, n)` — 按 Unicode 标量值截断
- `truncate_utf8_bytes(s, n)` — 字节截断确保边界合法
- `truncate_middle(s, head, tail)` — 中间省略
- `safe_preview(s, n)` — 日志预览

替换位置：Renderer、SessionChunker、AI Chunker、AI Client。

**测试**：b05_renderer_cjk_no_panic、b05_chunker_cjk_no_panic、b05_emoji_no_panic + 7个unit tests

---

#### B06 ✅ Secret Sanitizer 模式扩展
**问题**：`SecretKey=`、`token = `、`AWS_SECRET_ACCESS_KEY=`、`password="..."` 等未被脱敏；Unknown JSON 块未脱敏就写入 Markdown。

**修复**：
- 新增正则：`SecretKey`、`AccessKey`、`AWS_SECRET_ACCESS_KEY`、`AWS_ACCESS_KEY_ID`、引号包围的 token 值、`token = value` 带空格格式
- Renderer `Unknown` 块渲染前先调用 `self.sanitize()`

**测试**：b06_sanitizer_covers_common_patterns、b06_unknown_json_password_redacted + 7个unit tests

---

### M1：可靠采集与同步闭环

#### B02 ✅ SiYuan v3.8.3 API 契约修复
**问题**：
1. `createDocWithMd` 返回 `data` 为字符串 ID，当前按 `{id:...}` 解析导致失败
2. `setBlockAttrs`/`getBlockAttrs` 使用 `/api/block/` 路由，应为 `/api/attr/`
3. `find_document_by_session` 使用不存在的 `/api/search/searchAttr`

**修复**：
```rust
// 正确解析字符串 ID
match resp.data {
    Some(serde_json::Value::String(id)) => Ok(id),
    Some(serde_json::Value::Object(obj)) => /* 兼容旧格式 */
}
// 正确路由
"/api/attr/setBlockAttrs"
"/api/attr/getBlockAttrs"
```
`find_document_by_session` 改为返回 `None`，提示调用方使用本地 `sync_target` 中的 target_id 定位。

**测试**：b02_siyuan_string_id_accepted

---

#### B03 ✅ Sync hash 提前提交修复
**问题**：失败后第二次同步报 UNCHANGED（hash 已写入，但 sync_target 不存在或为 FAILED）。

**修复**：UNCHANGED 条件改为同时满足：
1. 内容 hash 匹配
2. sync_target 存在且状态为 SYNCED

```rust
let has_synced_target = match SyncTargetRepo::new(db).find(id, "siyuan") {
    Ok(Some(ref t)) => matches!(t.status, SyncStatus::Synced | SyncStatus::Unchanged),
    _ => false,
};
if has_synced_target { return SyncOutcome::Unchanged; }
// Otherwise: fall through and retry
```
dry-run 不再接触 sync_target 表。

**测试**：b03_failure_should_be_retried、b03_dry_run_no_poisoning

---

#### B11 ✅ 重提炼 FK 约束修复
**问题**：`save_items` 先删 `knowledge_item`，但 `knowledge_chunk` 有 FK 约束，导致外键失败。

**修复**：级联删除顺序 + 事务：
```rust
conn.execute_batch("BEGIN")?;
// embedding_record → knowledge_chunk → knowledge_fts → knowledge_item
conn.execute("DELETE FROM embedding_record WHERE chunk_id IN (SELECT id FROM knowledge_chunk WHERE knowledge_id = ?1)", ...)?;
conn.execute("DELETE FROM knowledge_chunk WHERE knowledge_id = ?1", ...)?;
conn.execute("DELETE FROM knowledge_fts WHERE knowledge_id = ?1", ...)?;
conn.execute("DELETE FROM knowledge_item WHERE source_session_id = ?1", ...)?;
// Insert new items...
conn.execute_batch("COMMIT")?;
```
失败时 ROLLBACK，上一版知识数据保持可用。

**测试**：b11_reextraction_no_fk_failure

---

#### B12 ✅ Worker 错误状态修复
**问题**：Provider 报错时 Worker 只打日志，pipeline_run 永久停留 PROCESSING。

**修复**：引入 `fail_stage!` 宏，在每个阶段失败时保证调用 `mark_failed()`：
```rust
macro_rules! fail_stage {
    ($stage:expr, $err:expr) => {{
        let _ = repo.mark_failed(run_id, $stage, &err_str);
        return Err(...);
    }};
}
```
外层 spawned task 也有安全网：
```rust
if let Err(e) = run_pipeline(...).await {
    let _ = repo.mark_failed(&job.pipeline_run_id, "UNKNOWN", &e.to_string());
}
```

**测试**：b12_worker_marks_failed_on_provider_error

---

#### B16 ✅ FTS 孤儿清理
**问题**：重提炼生成新 knowledge_id，旧的 FTS 行不被删除，导致搜索返回已删条目。

**修复**：`save_items` 在事务中显式删除旧 FTS 行：
```rust
conn.execute("DELETE FROM knowledge_fts WHERE knowledge_id = ?1", params![kid])?;
```

**测试**：b16_fts_no_orphans_after_reextraction

---

#### B19 ✅ Hash 覆盖范围扩展
**问题**：title、project、图片尾部不在 hash 中，内容变化检测不到。

**修复**：
```rust
hasher.update(b"title:"); hasher.update(title.as_bytes());
hasher.update(b"project:"); hasher.update(project.as_bytes());
// Image: full content (was limited to first 128 bytes)
hasher.update(source.as_bytes());
```

**测试**：b19_hash_includes_title、b19_hash_includes_full_image

---

#### B21 ✅ 知识列表过滤 SQL 修复
**问题**：`list_knowledge` 接受 `project`/`category` 参数但 SQL 无 WHERE 子句，total 也不过滤。

**修复**：动态构建 WHERE 子句，total 查询使用相同 predicate。

---

#### B23 ✅ Chunker 单消息过长处理
**问题**：单条超长消息（100k chars）产生的 chunk 超过 20k token 估算限制。

**修复**：当单 chunk 估算超过 `TARGET_TOKENS (20k)` 时，用 `truncate_chars` 截断：
```rust
const MAX_CHUNK_TOKENS: usize = TARGET_TOKENS; // 20k
if estimate_tokens(&raw_content) > MAX_CHUNK_TOKENS {
    let max_chars = ((MAX_CHUNK_TOKENS - 50) as f64 * CHARS_PER_TOKEN) as usize;
    // truncate...
}
```

**测试**：b23_single_long_message_within_limit

---

#### 003_pipeline_job.sql ✅ 持久化 Job 队列表
新增 `pipeline_job` 表，为 M2 可恢复 Pipeline 做准备：
```sql
CREATE TABLE pipeline_job (
    id, source, external_session_id, source_hash,
    generation, status, attempt, available_at, lease_until, ...
)
```

---

## 三、测试分层

| 层级 | 数量 | 说明 |
|------|------|------|
| Unit Tests (lib) | 92 | 模块内联测试，全部通过 |
| Correct-Behavior Tests (repro) | 15 | 基于审计 bug 编写，验证修复后的正确行为 |
| Integration / E2E | 0 | 待 M1 完成后补充 |

```
cargo test --workspace → 107 passed, 0 failed ✅
```

---

## 四、四层完成矩阵

| Bug | Implementation | Wired | Tests | Real E2E |
|-----|---------------|-------|-------|----------|
| B01 StateDb Mutex | ✅ | ✅ | ✅ | ❌ |
| B05 Unicode panic | ✅ | ✅ | ✅ | ❌ |
| B06 Sanitizer | ✅ | ✅ | ✅ | ❌ |
| B02 SiYuan API | ✅ | ✅ | ✅ | ❌ |
| B03 Sync hash | ✅ | ✅ | ✅ | ❌ |
| B11 FK cascade | ✅ | ✅ | ✅ | ❌ |
| B12 Worker FAILED | ✅ | ✅ | ✅ | ❌ |
| B16 FTS orphan | ✅ | ✅ | ✅ | ❌ |
| B19 Hash coverage | ✅ | ✅ | ✅ | ❌ |
| B21 Filter SQL | ✅ | ✅ | ❌ | ❌ |
| B23 Chunk limit | ✅ | ✅ | ✅ | ❌ |

Real E2E 需要配置的 AI 服务 (`http://127.0.0.1:11434/v1`) 和真实 SiYuan 环境，本次未执行。

---

## 五、尚未修复的项目（后续 M1/M2 继续）

| Bug | 状态 | 说明 |
|-----|------|------|
| B04 Conflict 检测 | ❌ | 需要读取远端正文计算 target_hash |
| B07 Tauri Runtime 替换 | ❌ | manage() 不替换已有状态 |
| B08 Watcher 生命周期 | ❌ | handle 离开作用域被 Drop |
| B09 Pipeline 自动入队 | 部分 | sync_and_enqueue 方法存在但未在所有入口调用 |
| B10 Settings 不生效 | ❌ | Engine 使用 Config::default() |
| B13 并发上限 | ❌ | 无 Semaphore |
| B14 rebuild-state 清空知识 | ❌ | 命令需要拆分 |
| B15 Missing Source | ❌ | mark_missing 未在生产调用 |
| B17 AI 失败语义 | ❌ | JSON 解析失败错误标记不准确 |
| B18 Provider 兼容 | ❌ | OpenCode Tool Error、Codex archived |
| B20 增量扫描 | ❌ | IncrementalScanner 仅单测调用 |
| B22 CLI 语义 | ❌ | dry-run/resync 语义不完整 |

---

## 六、后续计划

### M1 剩余（约 3-4 天）
1. B07: 修复 Tauri Runtime 单一所有权（`RwLock<Option<SiyuanRuntime>>`）
2. B08: Watcher lifecycle（持有 handle 到 AppState）
3. B09: 接通 sync_and_enqueue_extraction 所有入口（startup、button、watcher）
4. B10: Settings 真实生效（config 文件 → Engine 重建）
5. B04: 冲突检测（读远端正文计算 target_hash）

### M2（约 4-5 天）
1. B13: Semaphore 并发上限 + Session 单飞
2. 持久 Job 从 DB 恢复（启动时重载 PROCESSING → RETRY_WAIT）
3. Generation 机制（重提炼原子切换，旧版知识保留）
4. B17: AI 失败分类（NO_KNOWLEDGE / RETRYABLE / PERMANENT）

### M3（约 3 天）
1. Provider Golden Tests（四 Provider）
2. OpenCode WAL 并发测试
3. Windows 安装/升级/卸载验收
