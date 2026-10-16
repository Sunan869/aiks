# AI Knowledge Sync (AIKS)

[**简体中文**](README.md) · [**English**](README.en.md) · [Releases](https://github.com/Sunan869/aiks/releases) · [Third-party notices](THIRD_PARTY_NOTICES.md)

> **Local-first desktop knowledge management for AI-assisted work.** Import sessions from AI coding assistants and supported public conversation shares, distill structured knowledge through a durable pipeline, search across your work, and optionally sync documents with SiYuan.

## Features

| Feature | Details |
| --- | --- |
| Multi-provider sessions | Read local sessions from Claude Code, Codex CLI, Gemini CLI, OpenCode, WorkBuddy, Antigravity, Cursor, Cursor Agent, Cline, Roo Code, Kilo Code, GitHub Copilot, Kimi Code, Qwen Code, Continue, and Aider; selected **public conversation share URLs** can also be imported |
| Local-first storage | Source files are read-only; SQLite persists session identities, processing state, knowledge records, and search indexes. Personal offline operation does not require a cloud account |
| Durable extraction | Configurable OpenAI-compatible models, persistent queue, stage tracking, retries, crash recovery, diagnostics, and review workflows |
| Knowledge workspace | Structured knowledge items, projects, categories, tags, feedback, organization, and relations |
| Search and Q&A | Full-text search with optional embeddings/semantic hybrid retrieval and graceful degradation to text retrieval |
| SiYuan integration | Bundled SiYuan Kernel, document synchronization, tracked mappings, ownership/content-hash conflict protection; no direct edits to SiYuan's internal database |
| Processing center | Unified task list with database-wide search and pagination, stage inspection, status filters, and separately labeled session/pipeline metrics |
| Void and restore | Voiding isolates a session from search and new automatic work, then safely removes AIKS-managed SiYuan documents, derived knowledge, and indexes while preserving source identity for manual restoration and re-extraction |

**Compatibility disclaimer:** Support depends on each provider's specific session format and version. Not every historical or future format is supported. See the [provider compatibility notes](docs/reference-analysis/multi-provider-support.md). Public share formats may change; import only content you are authorized to access.

## How it works

```text
Local coding-agent sessions / supported public conversation shares
                            │ read-only import
                            ▼
                    Canonical Session
                            ▼
                 SQLite + durable job queue
                            ▼
            Cleaning / chunking / AI extraction
                            ▼
                     KnowledgeItem
                        /       \
                      FTS    Embeddings (optional)
                        \       /
                         Search
                            │
                 Desktop / CLI / Ask AIKS
                            │
                    SiYuan sync (optional)
```

SiYuan is a document runtime and a knowledge export/sync target, **not** the only storage layer. AIKS owns its canonical session, knowledge, task, and retrieval state. User-edited SiYuan documents are never silently overwritten or deleted on a detected conflict.

## Installation and first steps

Download the relevant Windows, macOS, or Linux package from [GitHub Releases](https://github.com/Sunan869/aiks/releases). Supported package types depend on the assets attached to each release.

1. Open **Data Sources** and enable the providers and directories you need.
2. Configure an AI endpoint and optional embedding endpoint in **Settings**. **AI and embeddings are disabled by default**, and imported data can be browsed without them.
3. Scan/synchronize sources, inspect task stages in **Processing Center**, preview conversations in **Work Records**, and explore extracted items in the knowledge workspace.
4. Test document sync and void/restore with **isolated test data** first. Voiding is a destructive operation **for AIKS-managed SiYuan documents and derived knowledge**; ownership and content conflicts block unsafe cleanup.

Voiding **does not delete a provider's original local conversation files**. A voided source is excluded from automatic re-import; restoration allows a fresh source scan and re-extraction. Some public-share sources require the original link to be imported again.

## Build from source

Requirements: Rust stable, Node.js 22+, npm, and your operating system's [Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/).

```bash
cargo build --workspace
cargo test --workspace

cd apps/aiks-desktop
npm ci
npm run build
npm run tauri dev
```

Windows PowerShell development example:

```powershell
$env:AIKS_DATA_DIR = "E:\\AIKS-Test\\data"
Remove-Item Env:AIKS_BACKEND_MODE -ErrorAction SilentlyContinue
.\\scripts\\dev.ps1
```

When rebuilding, shut down an older AIKS/SiYuan instance first if its running `SiYuan-Kernel.exe` locks files under `target/debug/resources`. Do **not** delete production data folders to resolve a build lock.

Project layout: `crates/aiks-core/` (Rust core, providers, SQLite, pipeline, search), `apps/aiks-desktop/` (React/Vite/Tauri), `apps/aiks-cli/` (CLI), `docs/` (design, references, compatibility). See `config.example.toml` for configuration and `AGENTS.md` for repository conventions.

```bash
cargo run -p aiks-cli -- --help
cargo run -p aiks-cli -- --config ./config.toml doctor
```

Never commit API keys, tokens, user conversation content, private file paths, or unredacted diagnostics.

## Acknowledgments and upstream projects

AIKS builds on and draws inspiration from many excellent projects. Some session and public-share parsing implementations also build on upstream work. Particular thanks to:

- [SiYuan](https://github.com/siyuan-note/siyuan) — bundled knowledge document runtime and public HTTP API.
- [AICoder Session Viewer](https://github.com/seastart/aicoder-session-viewer) — important early reference and adapted source for Claude/Codex/Gemini/OpenCode session parsers.
- [CC Switch](https://github.com/farion1231/cc-switch) and [ccusage](https://github.com/ccusage/ccusage) — approaches to source discovery, session scanning, and usage information.
- [OpenAI Codex](https://github.com/openai/codex), [Gemini CLI](https://github.com/google-gemini/gemini-cli), [OpenCode](https://github.com/anomalyco/opencode), and [Claude Code](https://github.com/anthropics/claude-code) — official format/behavior references.
- [chatgpt-share-parser](https://github.com/evanhu1/chatgpt-share-parser), [chat-share-reader](https://github.com/pencil311/chat-share-reader), and other public-conversation parsing references.
- [Tauri](https://github.com/tauri-apps/tauri), [React](https://github.com/facebook/react), [Radix UI](https://github.com/radix-ui/primitives), and the Rust ecosystem — desktop and UI foundations.

The projects and trademarks belong to their respective owners. Acknowledgment does **not** imply sponsorship, affiliation, or endorsement. Detailed project references, pinned versions, and notes on adapted code are in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

## Contributions and disclaimer

- Contributions and reproducible issue reports are welcome. Please read `AGENTS.md` before contributing.
- Software is supplied without warranty. Back up important data before voiding, rebuilding, or upgrading; AI-generated knowledge must be reviewed for accuracy.

Thanks to every upstream maintainer, contributor, and tester who makes this ecosystem possible.
