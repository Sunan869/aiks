"""Final scoped PR #46 edits; delete this helper and its write workflow before review."""
from pathlib import Path
import subprocess


def edit(path, old, new):
    file = Path(path)
    text = file.read_text(encoding="utf-8")
    if new in text:
        return
    if text.count(old) != 1:
        raise RuntimeError("Expected one anchor in " + path)
    file.write_text(text.replace(old, new, 1), encoding="utf-8")
    print("Updated", path)


# The merged base introduced a new Clippy warning. This is the equivalent
# operation with the same positive, compile-time candidate cap, not a search change.
edit("crates/aiks-core/src/search/lexical.rs",
     "requested_limit.max(1).min(CANDIDATE_CAP)",
     "requested_limit.clamp(1, CANDIDATE_CAP)")

section = """## 多工具本地会话接入

新增原生只读来源：Antigravity、Cursor、Cursor Agent、Cline、Roo Code、Kilo Code、GitHub Copilot、Kimi Code、Qwen Code、Continue、Aider。它们与原有五个来源复用 Canonical Session、同步、持久任务、知识提炼和搜索，不依赖安装另一个会话查看器。

**支持范围不是各工具的所有历史版本。** Antigravity 当前可导入已有 CLI transcript；只有 IDE token/usage 缓存时会明确显示格式不支持，绝不把统计数据拼成用户/助手对话。Cursor Agent 当前支持 JSONL，Kimi 覆盖主 Agent wire 与旧 context 布局，Copilot 覆盖 CLI/Desktop 本地事件和 VS Code 会话快照/补丁。Aider 必须显式配置项目根目录。

数据源页面统一展示正式名称、状态和目录配置；保存目录/开关后需要重启。来源稳定 key 不随品牌名变化，关闭来源或移除配置根不会删除已导入记录。自动化样例与实际安装环境验收分别记录；具体文件布局、测试入口和已知限制见 `docs/reference-analysis/multi-provider-support.md`。

"""
edit("README.md", "## 当前能力\n", section + "## 当前能力\n")
edit("README.md",
     "- Claude Code / Codex / Gemini CLI / OpenCode / WorkBuddy Session Provider。",
     "- 原有五个 Session Provider，加上上述 11 个本地来源适配器；精确支持范围见支持矩阵。")

rules = """### 新增本地来源（PR #46）

Antigravity、Cursor、Cursor Agent、Cline、Roo Code、Kilo Code、GitHub Copilot、Kimi Code、Qwen Code、Continue、Aider 通过 `providers/native.rs` 与各格式模块接入。旧五个来源身份保持不变，来源显示和筛选统一消费 Core catalog；Share URL 缓存来源仍与本地目录来源区分。

- 新来源只经 `ScopedReader` 读取允许的 transcript 和必要元数据；加载时复查来源身份及路径边界，拒绝逃逸链接/reparse points，并保留读取预算和完整性诊断。
- Cursor 与 Kilo 索引 SQLite 只读且读取 WAL；不得枚举与会话无关的配置值或凭据。
- 部分扫描保留有效会话，但不能据此将未扫描/损坏/禁用来源的旧记录标为缺失。
- Antigravity IDE 的 usage/token 缓存不是对话，不得用它伪造消息。Aider 不默认扫描用户主目录或整个磁盘。
- Provider tests 必须显式配置 AI/Embedding 或使用回环测试服务；不得依赖部署模型默认值，也不得为了测试通过修改部署配置。
- `multi_provider_acceptance` 对 11 个适配器分别运行实际同步、去重、提炼、发布与混合搜索；真实安装环境和版本兼容范围仍单独验收。参见 `docs/reference-analysis/multi-provider-support.md`。

"""
edit("AGENTS.md", "### State / Pipeline\n", rules + "### State / Pipeline\n")

old = """新增 Provider 前先确认真实需求，并延续只读 source + canonical model 边界。候选包括：

- [ ] Cursor。
- [ ] Windsurf。
- [ ] Copilot CLI。
- [ ] Qwen Code / Kimi 等本地或 CLI Agent。"""
new = """PR #46 的 11 个本地来源已有原生适配器，精确格式与自动化证据见 `docs/reference-analysis/multi-provider-support.md`；不再将同一批来源作为尚未开始的重复开发任务。

- [ ] 对上述来源完成真实安装环境与版本矩阵验收（尤其 Windows 自动目录发现）。
- [ ] Antigravity IDE 的真实消息存储：当前仅统计缓存不支持，不把 usage 伪造为对话。
- [ ] 尚未覆盖的历史/未来格式按匿名 fixture 增量适配；Cursor Agent 非 JSONL、新 Kimi 子 Agent 等不因来源名称已注册就视为已验证。
- [ ] Windsurf 仍是单独候选，不属于本次 11 个来源范围。"""
edit("TODO.md", old, new)

# Stage only the explicitly reviewed documentation paths; the workflow's
# existing allowlist stages code separately. Never add arbitrary working files.
subprocess.run(["git", "add", "--", "README.md", "AGENTS.md", "TODO.md"], check=True)
