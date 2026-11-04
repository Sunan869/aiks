# AIKS SEO keyword and content plan

Updated: 2026-11-04. This is a content/information-architecture plan, not a claim of live Google positions.

## Target queries and landing pages

| Search intent | Locale | Primary query | Landing page |
| --- | --- | --- | --- |
| Find Claude Code local chats | zh-CN | Claude Code 对话记录管理 | /aiks/guides/claude-code-session-history/ |
| Search Codex CLI sessions | zh-CN | Codex CLI 历史会话搜索 | /aiks/guides/codex-cli-session-history/ |
| Recover Cursor Composer chats | zh-CN | Cursor Composer 历史记录 | /aiks/guides/cursor-composer-history/ |
| Claude Code history search | en | Claude Code session history search | /aiks/en/guides/claude-code-session-history/ |
| Codex CLI conversation search | en | Codex CLI conversation history viewer | /aiks/en/guides/codex-cli-session-history/ |
| Cursor Composer history | en | Cursor Composer history search | /aiks/en/guides/cursor-composer-history/ |

## Architecture

- Two existing homepages remain primary product entry points. Each links to its guide hub and each specific tutorial.
- Two guide hubs link to three search-intent tutorials in the same language.
- Every tutorial links to both source-compatible neighbors, the homepage, Downloads, and the existing provider support matrix.
- Every page has its own title, description, canonical URL, reciprocal zh-CN/en hreflang and indexable HTML content.
- `docs/sitemap.xml` includes all ten public URLs with language alternatives.
- `scripts/check-site-seo.mjs` verifies links and metadata from CI before changes land.

## Accuracy constraints

- Cursor Composer SQLite and Cursor Agent JSONL are **distinct** supported local provider identities.
- Format/version support is bounded by actual parser coverage; never promise all possible agent/chat histories.
- AI extraction, semantic search and model services are opt-in; local providers are read-only.
- Do not imply third-party affiliation, automatic cloud account access or guaranteed search rankings.

## Tracking and measurement

Once Google Search Console can read the `https://sunan869.github.io/aiks/` URL-prefix property, submit `https://sunan869.github.io/aiks/sitemap.xml` once and inspect real indexing status. Evaluate non-brand query impressions, clicks, CTR, and average position per page over subsequent weeks. Do not use isolated `site:` searches as a definitive indexing report.

Windsor.ai can act as an optional reporting connector if both ChatGPT and the user's Search Console property are connected; it does **not** accelerate crawling, indexing or rankings.
