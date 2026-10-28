# AIKS · AI Knowledge Sync

[简体中文](README.md) · **English** · [Download](https://github.com/Sunan869/aiks/releases) · [Provider compatibility](docs/reference-analysis/multi-provider-support.md) · [Third-party notices](THIRD_PARTY_NOTICES.md)

> ### Your AI conversations should not be disposable.
> **16 local AI coding tools + 8 public web conversation-share sources = 24 source types.** Collect supported AI sessions, extract structured knowledge, connect projects, search with full-text or optional semantic retrieval, and sync with SiYuan. Turn every AI-assisted task into experience you can reuse.

**24 source types** · **Local-first** · **Read-only local providers** · **AI knowledge extraction** · **Full-text + optional semantic search** · **SiYuan integration** · **MIT-licensed / commercial use allowed**

[**Download AIKS**](https://github.com/Sunan869/aiks/releases) · [Source code](https://github.com/Sunan869/aiks) · [Report an issue](https://github.com/Sunan869/aiks/issues)

## 🌐 Supported AI data sources

Instead of copying chats from multiple assistants into a note-taking app, import supported local sessions and public web conversation shares into one knowledge workflow.

### 16 local coding assistant and agent sources

| | | | |
| --- | --- | --- | --- |
| Claude Code | OpenAI Codex CLI | Gemini CLI | OpenCode |
| Cursor | Cursor Agent | GitHub Copilot | Antigravity |
| Cline | Roo Code | Kilo Code | WorkBuddy |
| Kimi Code | Qwen Code | Continue | Aider |

**Local providers are opt-in.** AIKS reads source histories without modifying upstream files. Actual support varies by version, storage layout, and path. Review the [provider compatibility matrix](docs/reference-analysis/multi-provider-support.md).

### 8 public web conversation-share sources

| | | | |
| --- | --- | --- | --- |
| ChatGPT | Claude | Gemini | DeepSeek |
| Doubao | Kimi | Tencent Yuanbao | Qwen |

Import supported **public share URLs** without connecting your accounts. Web page changes may affect individual parsers. This is **not** automatic access to all private browser or account chat history.

### From conversation to reusable knowledge

```text
16 local agent sources        8 public web share sources
              \              /
                Unified import
                     ↓
         Cleaning / chunking / AI extraction
                     ↓
     Knowledge · Tags · Projects · Provenance
                     ↓
         Full-text / Semantic / Ask AIKS
                     ↓
             SiYuan sync (optional)
```

AI extraction and embeddings are optional and disabled by default. Imported sessions remain browsable without a configured model. SiYuan sync protects against overwriting user edits.

---
## Why AIKS?

AI assistants help us debug incidents, discuss architecture, write code, and work through complex decisions. Yet the useful reasoning is often scattered across tools and workspaces. **The task gets done, but the experience remains buried in a conversation.** Weeks later, we repeat the search, rebuild context, and solve the same problem again.

AIKS is more than a combined chat history viewer. It turns AI-assisted work into knowledge you can actually reuse:

- **Recover the process:** Read historical sessions from multiple coding assistants without manually copying conversations.
- **Extract the outcome:** Turn long discussions into structured problems, findings, steps, and solutions, not merely raw chat archives.
- **Preserve context:** Associate knowledge with original sessions, projects, and documents so conclusions can be traced back.
- **Reuse what you learned:** Find prior answers through full-text search, optional semantic retrieval, and knowledge Q&A.
- **Stay in control:** Keep data local-first, inspect every processing stage, manage incorrect knowledge, and void/rebuild AIKS-managed content when appropriate.

> **From “AI helped me finish this task” to “Every AI-assisted task contributes to experience I can reuse.”**

## What makes AIKS different?

### 01 · Many assistants, one work-history view

Session formats differ across tools. AIKS uses Provider adapters to normalize supported local histories and selected public share conversations into a Canonical Session model. Browse, preview, filter, and locate source context in **Work Records** without hopping between apps.


### 02 · Knowledge extraction, not just archiving

Importing a conversation is only the beginning. AIKS uses a durable pipeline for parsing, cleaning, chunking, AI extraction, and optional embedding. Knowledge records have structured fields such as titles, categories, summaries, content, and tags. **A lengthy debugging conversation can become a findable, maintainable solution.**

The **Processing Center** exposes task stages and failures, so background processing does not remain a black box.


### 03 · Find ideas even when you forget the exact words

AIKS provides full-text search and optional embedding-based semantic retrieval, with knowledge reuse through its workspace and **Ask AIKS**. When an embedding service is unavailable, text search still works.


### 04 · Knowledge with provenance and lifecycle

Generated knowledge is not assumed to be permanently correct. Organize records by project, category, and tags, provide human feedback, and manage relations. SiYuan integration provides document access and sync with ownership and content-hash conflict protection.

**Void and restore** goes beyond hiding a result: it isolates a session from automatic work, safely cleans up eligible AIKS-managed documents, extracted knowledge, and indexes, while retaining source identity for a later manual re-import. Conflicts halt cleanup rather than deleting user edits.


### 05 · Local-first with observable processing

AIKS maintains its state, knowledge, and retrieval indexes in local SQLite and reads provider files without rewriting them. Scanning can be manual or periodic. AI and embedding endpoints are optional and disabled by default.

AIKS primarily addresses **personal knowledge accumulated through AI-assisted work**. It is not presented as a finished enterprise-wide collaboration and authorization platform.

## Practical use cases

| Scenario | Workflow | Benefit |
| --- | --- | --- |
| Engineering debugging | Distill a multi-step incident investigation from a Session | Recover diagnostics, causes, and fixes next time |
| Architecture and design | Connect design discussions to a project | Reuse trade-offs and the reasoning behind decisions |
| Testing and quality | Capture reproduction steps, test plans, and fix analysis | Build a searchable bank of practical cases |
| Learning and research | Import useful AI discussions and supported public shares | Preserve conclusions and source context |
| Cross-assistant workflows | Work across different coding assistants | Search past experience from one place |

**Typical workflow:** Enable sources → scan sessions → inspect extraction → review knowledge → search and reuse → revise or void as needed.

## Detailed features

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

### macOS: install and launch

1. Download the `.dmg` for your Mac from [Releases](https://github.com/Sunan869/aiks/releases): choose `aarch64/arm64` for Apple Silicon (M-series) or `x64` for an Intel Mac.
2. Open the DMG and drag **AIKS.app** into **Applications**.
3. Launch AIKS from Finder → Applications.

**Unsigned builds:** Current macOS releases have not been signed with an Apple Developer ID and notarized by Apple. Gatekeeper may block the first launch or warn that macOS cannot verify whether AIKS contains malware. Verify the download came from the linked GitHub release and matches your platform. Check **System Settings → Privacy & Security** for an **Open Anyway** option for AIKS.

Only if you trust this specific release and accept the risks of bypassing that check, run the following in Terminal:

```bash
sudo xattr -dr com.apple.quarantine /Applications/AIKS.app
open /Applications/AIKS.app
```

This removes the app's quarantine attribute; it **does not sign or notarize AIKS**. Do not run the command on untrusted software and do not disable Gatekeeper system-wide. Adjust the path if AIKS is installed elsewhere. If it still fails, report the macOS version, CPU architecture, and error message in an Issue.

### Windows / Linux

On Windows, use the `.exe` installer or `.msi` for your platform. On Linux, use the appropriate `.deb` or `.AppImage`; an AppImage may require `chmod +x <filename>.AppImage` before launch.

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
