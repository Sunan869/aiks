# Third Party Notices

> 本文件记录所有复制、修改、移植的第三方代码，以及随 AIKS 分发的开源组件。
> 必须包含来源仓库、固定 Commit SHA/Version、License 和衍生/分发文件路径。

## SiYuan — Bundled Runtime (AGPL-3.0)

**重要：AIKS Desktop 将 SiYuan Kernel 及其运行资源内嵌于安装包中分发。**

- Repository: https://github.com/siyuan-note/siyuan
- Bundled Version: **3.8.3**
- Release Tag: v3.8.3
- Windows Asset: siyuan-3.8.3-win.exe
- SHA256: 49cd67c6e892aff5a04189707007df3046d48c0dc279840433b9824edd374569
- Checked Date: 2026-09-11
- License: **AGPL-3.0**
- Copyright: SiYuan contributors (https://github.com/siyuan-note/siyuan/graphs/contributors)
- Usage: Embedded knowledge base engine — SiYuan-Kernel.exe is bundled as a runtime and started as a child process
- Bundled Files:
  - `apps/aiks-desktop/src-tauri/resources/siyuan/kernel/SiYuan-Kernel.exe`
  - `apps/aiks-desktop/src-tauri/resources/siyuan/stage/`
  - `apps/aiks-desktop/src-tauri/resources/siyuan/appearance/`
  - `apps/aiks-desktop/src-tauri/resources/siyuan/guide/`
- Source Code: https://github.com/siyuan-note/siyuan
- Notes: |
  AIKS does NOT modify SiYuan source code. SiYuan is integrated solely via its
  HTTP API (localhost only). The kernel is started as an isolated child process.
  Per AGPL-3.0 requirements: License and Copyright notices are preserved.
  Source code is available at the above repository. AIKS does not "use" SiYuan
  as a library; it distributes the official unmodified binary as a bundled runtime.

## AICoder Session Viewer

- Repository: https://github.com/seastart/aicoder-session-viewer
- Commit SHA: b750098594c3bb969d5039a9f2e44a283c38af9b
- Checked Date: 2026-09-11
- License: MIT
- Usage: Session Provider / Parser primary source — all four providers (Claude, Codex, Gemini, OpenCode) adapted from this repo
- Derived Files:
  - `src/providers/claude.rs` — adapted from `src-tauri/src/providers/claude.rs`
  - `src/providers/codex.rs` — adapted from `src-tauri/src/providers/codex.rs`
  - `src/providers/gemini.rs` — adapted from `src-tauri/src/providers/gemini.rs`
  - `src/providers/opencode.rs` — adapted (with live schema updates) from `src-tauri/src/providers/opencode.rs`
- Notes: |
  MIT License notice preserved. AIKS rewrites providers to output AIKS Canonical Model,
  adds async_trait wrapping, live-schema-confirmed OpenCode parser, and incremental sync state.

## CC Switch

- Repository: https://github.com/farion1231/cc-switch
- Commit SHA: 7726c83476f9ae1f8a5b812aa844cd166339aa55
- Checked Date: 2026-09-11
- License: MIT
- Usage: Reference for session scanning, path discovery, incremental state patterns
- Derived Files: None (reference only, no code copied)

## ccusage

- Repository: https://github.com/ccusage/ccusage
- Commit SHA: 2feea4da2a8f2a8002db7fc297a78c0cdb4c0027
- Checked Date: 2026-09-11
- License: MIT
- Usage: Reference for multi-agent data sources, future Provider expansion
- Derived Files: None (reference only)

## Mnemos

- Repository: https://github.com/mnemos-dev/mnemos
- Commit SHA: TBD (not yet analyzed — V1.5 concern)
- Checked Date: N/A
- License: MIT
- Usage: Knowledge distillation / session → KnowledgeItem design reference (V1.5)
- Derived Files: None yet

## SiYuan

- Repository: https://github.com/siyuan-note/siyuan
- Commit SHA: N/A (used via HTTP API only, no code copied)
- Checked Date: 2026-09-11
- License: AGPL-3.0
- Usage: External knowledge platform integrated through HTTP API only
- Derived Files: None — AIKS does not copy or embed SiYuan source code

## OpenAI Codex

- Repository: https://github.com/openai/codex
- Commit SHA: eaa8b6d91701d6cabe464141facc677e5915fbfc
- Checked Date: 2026-09-11
- License: Apache 2.0
- Usage: Official source of truth for Codex session file format and schema
- Derived Files: None (reference only — format information used in codex.rs)

## Gemini CLI

- Repository: https://github.com/google-gemini/gemini-cli
- Commit SHA: ed2ac40df67a319bf348bd7e3d10494696b31b38
- Checked Date: 2026-09-11
- License: Apache 2.0
- Usage: Official source of truth for Gemini CLI session file format
- Derived Files: None (reference only — format information used in gemini.rs)

## OpenCode

- Repository: https://github.com/anomalyco/opencode
- Commit SHA: 193de13a88d62a6409c6d385831180f1def527dc
- Checked Date: 2026-09-11
- License: MIT
- Usage: Official source of truth for OpenCode SQLite schema and migrations
- Derived Files: None (reference only — schema confirmed via live DB inspection)

## Claude Code

- Repository: https://github.com/anthropics/claude-code
- Commit SHA: N/A (proprietary, not publicly available as source)
- Checked Date: 2026-09-11
- License: Anthropic proprietary
- Usage: Official source of truth for Claude Code JSONL format and behavior changes
- Derived Files: None (format information used in claude.rs, cross-verified with live files)

## Rust Crate Licenses

The following crates are used under their respective licenses:

| Crate | License | Version |
|---|---|---|
| tokio | MIT | 1.x |
| serde | MIT/Apache-2.0 | 1.x |
| serde_json | MIT/Apache-2.0 | 1.x |
| rusqlite | MIT | 0.31.x |
| reqwest | MIT/Apache-2.0 | 0.12.x |
| tracing | MIT | 0.1.x |
| tracing-subscriber | MIT | 0.3.x |
| clap | MIT/Apache-2.0 | 4.x |
| tauri | MIT/Apache-2.0 | 2.x |
| tauri-plugin-autostart | MIT/Apache-2.0 | 2.x |
| tauri-plugin-shell | MIT/Apache-2.0 | 2.x |
| tauri-plugin-window-state | MIT/Apache-2.0 | 2.x |
| notify | CC0-1.0 | 6.x |
| notify-debouncer-full | CC0-1.0 | 0.3.x |
| sha2 | MIT/Apache-2.0 | 0.10.x |
| hex | MIT/Apache-2.0 | 0.4.x |
| uuid | MIT/Apache-2.0 | 1.x |
| rand | MIT/Apache-2.0 | 0.8.x |
| chrono | MIT/Apache-2.0 | 0.4.x |
| thiserror | MIT/Apache-2.0 | 1.x |
| anyhow | MIT/Apache-2.0 | 1.x |
| walkdir | MIT/Unlicense | 2.x |
| async-trait | MIT/Apache-2.0 | 0.1.x |
| flate2 | MIT/Apache-2.0 | 1.x |
| dirs | MIT/Apache-2.0 | 5.x |
| regex | MIT/Apache-2.0 | 1.x |
| tempfile | MIT/Apache-2.0 | 3.x |
| tracing-subscriber | MIT | 0.3.x |
| clap | MIT/Apache-2.0 | 4.x |
| toml | MIT/Apache-2.0 | 0.8.x |
| notify | CC0-1.0 | 6.x |
| sha2 | MIT/Apache-2.0 | 0.10.x |
| uuid | MIT/Apache-2.0 | 1.x |
| chrono | MIT/Apache-2.0 | 0.4.x |
| thiserror | MIT/Apache-2.0 | 1.x |
| anyhow | MIT/Apache-2.0 | 1.x |
| walkdir | MIT/Unlicense | 2.x |
| async-trait | MIT/Apache-2.0 | 0.1.x |
| flate2 | MIT/Apache-2.0 | 1.x |
| dirs | MIT/Apache-2.0 | 5.x |
| regex | MIT/Apache-2.0 | 1.x |
| hex | MIT/Apache-2.0 | 0.4.x |
| tempfile | MIT/Apache-2.0 | 3.x |


## chatgpt-share-parser

- Repository: https://github.com/evanhu1/chatgpt-share-parser
- Checked Date: 2026-09-21
- License: MIT
- Copyright: Copyright (c) 2026 Evan Hu
- Usage: ChatGPT public Share URL parser (modern React Flight + legacy Next.js payloads)
- Derived Files:
  - `apps/aiks-desktop/src/share-import/chatgpt-share-parser.ts`
- Notes: |
  AIKS vendors and adapts the MIT parser. Payload lookup was additionally
  hardened to discover conversation objects by shape instead of relying only
  on one route key.

## chat-share-reader

- Repository: https://github.com/pencil311/chat-share-reader
- Checked Date: 2026-09-21
- License: MIT (declared in package.json)
- Usage: Reference and adapted selector strategy for browser-side Claude public Share URL extraction
- Derived Files:
  - `apps/aiks-desktop/src-tauri/src/share_import_commands.rs`
- Notes: |
  AIKS uses its own local Tauri WebView flow rather than the project's hosted
  MCP service/bookmarklet. Claude selector and browser-extraction concepts are
  adapted from the MIT project.

## AI Chat Exporter

- Repository: https://github.com/TheBluCoder/AI-chat-exporter
- Checked Date: 2026-09-21
- License: MIT
- Copyright: Copyright (c) 2024 AI Chat Exporter Contributors
- Usage: Gemini public shared-conversation browser selector concepts
- Derived Files:
  - `apps/aiks-desktop/src-tauri/src/share_import_commands.rs`

### MIT License (Share URL parser dependencies)

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The applicable copyright notice and this permission notice shall be included
in all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

## Local AI Session Provider References and Adapted Strategies (PR #46)

### Claude Code History Viewer

- Repository: https://github.com/jhlee0409/claude-code-history-viewer
- Commit SHA: fdfc766ce7f0d76dceb03087aedac47add33d61b
- Checked Date: 2026-09-21
- License: MIT
- Copyright: Copyright (c) 2025 JaeHyeok Lee
- Usage: Reference storage layouts and adapt parsing strategies for the eleven local AI session sources. AIKS uses its own bounded read-only IO, Canonical Model and downstream pipeline; the viewer application is not bundled.
- Associated native adaptation files under `crates/aiks-core/src/providers/`:
  - `antigravity.rs`, `cursor/mod.rs`, `cursor_agent.rs`
  - `cline_family.rs` (Cline / Roo Code / Kilo Code)
  - `copilot/mod.rs`, `kimi/mod.rs`, `kimi/wire.rs`
  - `qwen.rs`, `continue_dev.rs`, `aider.rs`
  - `local_paths.rs`, `message_parts.rs`
- Notes: This attributes format and parser strategies, not a claim that whole reference modules were vendored. Antigravity IDE token statistics are deliberately not converted into synthetic dialogue. Exact supported layouts and tests are recorded in `docs/reference-analysis/multi-provider-support.md`.

### Kimi Code

- Repository: https://github.com/MoonshotAI/kimi-code
- Commit SHA: 6a214b85e53e58a9ef6480f27bcb7b0103c0e34e
- Checked Date: 2026-09-21
- License: MIT
- Copyright: Copyright (c) 2026 Moonshot AI
- Usage: Official session/wire event semantics reference for the native adapters in `crates/aiks-core/src/providers/kimi/`. Kimi executables, models and account credentials are not bundled or read by these adapters.

### MIT License (Local Provider References)

Copyright (c) 2025 JaeHyeok Lee
Copyright (c) 2026 Moonshot AI

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.


## doubao-nomark

- Repository: https://github.com/ihmily/doubao-nomark
- Checked Date: 2026-09-21
- License: MIT
- Copyright: Copyright (c) 2026 Hmily
- Usage: Public Doubao and Qwen share URL formats, Qwen share-info API shape, and Doubao share-page loader structure reference
- Derived Files:
  - `apps/aiks-desktop/src-tauri/src/share_import_commands.rs`
- Notes: |
  AIKS adapts the public-share extraction concepts into its own Tauri WebView
  importer and normalized session schema. It does not bundle the upstream
  browser extension or service.

## OpenCLI — Yuanbao Browser Structure Reference

- Repository: https://github.com/jackwener/OpenCLI
- Checked Date: 2026-09-21
- License: Apache-2.0
- Usage: Tencent Yuanbao rendered-message DOM structure and role-bearing container reference
- Derived Files:
  - `apps/aiks-desktop/src-tauri/src/share_import_commands.rs`
- Notes: |
  AIKS implements its own read-only public-share extractor. The upstream
  project is used as a compatibility reference for Yuanbao's rendered DOM.

## yuanbao_ref_link

- Repository: https://github.com/engrecho/yuanbao_ref_link
- Checked Date: 2026-09-21
- License: MIT
- Copyright: Copyright (c) 2025 Jaylon
- Usage: Confirmation of the public `https://yb.tencent.com/s/*` share surface and rendered reference-area conventions
- Derived Files: None (reference only)
