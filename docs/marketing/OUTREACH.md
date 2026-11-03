# AIKS discovery and directory submissions / 开源项目推广清单

As of 2026-11-03, the following lists were checked for scope and submission rules. **No third-party PRs have been submitted as part of this file.** Check the latest rules again before submitting, use factual copy, and disclose your relationship to the project when relevant.

## Shortlist (do not mass-submit)

### 1. scortt/awesome-ai-dev-tools — good topical match

- Repository: https://github.com/scortt/awesome-ai-dev-tools
- Contributing: https://github.com/scortt/awesome-ai-dev-tools/blob/main/CONTRIBUTING.md
- Section: **Docs, Knowledge, And RAG**.
- Proposed one-line entry:

```markdown
- [AIKS](https://github.com/Sunan869/aiks) - Local-first desktop knowledge base that reads coding-agent session histories and extracts searchable, source-linked knowledge. `Rust` `MIT`
```

- Why: Its submission guidelines explicitly cover knowledge tools supporting AI-assisted engineering.
- Before PR: verify whether an AIKS entry already exists, confirm current license, ensure the website/screenshots and download work. Create a fork and a *single-project PR*.

### 2. pedramnj/awesome-local-ai — possible match, subject to review

- Repository: https://github.com/pedramnj/awesome-local-ai
- Contributing: https://github.com/pedramnj/awesome-local-ai/blob/main/CONTRIBUTING.md
- Section: **RAG & Knowledge**.
- Proposed one-line entry:

```markdown
- [AIKS](https://github.com/Sunan869/aiks) — Local-first desktop app for turning supported coding-agent histories and public conversation shares into searchable, source-linked knowledge.
```

- Must clearly explain the distinction: AIKS is a session-knowledge workflow, **not** a local-model runtime; AI processing is optional and must be explicitly configured. Do not claim that every configuration keeps inference offline.

### 3. pandego/awesome-agentic — manual editorial consideration

- Repository: https://github.com/pandego/awesome-agentic
- Section: **Local-first and personal memory**.
- Strong editorial curation, and AIKS is not an agent-side memory API; submit only if the maintainers consider personal knowledge derived from coding-agent sessions in scope. A scoped proposal rather than mass submission is preferable.

## Do NOT submit

- `subinium/awesome-claude-code`: requires **1,000+ stars**; AIKS does not meet that threshold at this time.
- `awesome-selfhosted/awesome-selfhosted-data`: rejects LLM-generated submissions; its eligibility rules also do not clearly fit an opt-in personal desktop app.
- Strictly local-inference-only lists: AIKS supports user-configured remote inference, so avoid advertising it as guaranteed on-device AI.

## Submission checklist

1. Verify list rules, section scope, license and active maintenance.
2. Read the latest repo README and check for an existing `AIKS` entry.
3. Use a factual, short description, no star claims, no unsupported compatibility promises.
4. Limit each PR to one entry in one list; declare affiliation when asked.
5. Check CI and respond to maintainers' feedback.
6. Track PR URL, review and merge state; **an opened PR is not a published listing**.

## Where to share (editorial distribution, not directories)

- Short GitHub repository overview linking to the bilingual landing page and real screenshots.
- A practical engineering post about how AIKS extracts knowledge from past Claude Code/Codex/Cursor sessions; clearly mark that sources are read-only and web-share imports are public-link only.
- Post a short screen-capture clip **only after** recording or producing a faithful demo; a montage from still screenshots must be labeled a screenshot tour rather than a real workflow recording.

## GitHub Pages setup (one-time manual account setting)

1. Open https://github.com/Sunan869/aiks/settings/pages
2. **Build and deployment → Source: Deploy from a branch**.
3. **Branch: `main`; Folder: `/docs`**. Click **Save**.
4. Check https://sunan869.github.io/aiks/ and https://sunan869.github.io/aiks/en/ after the deployment is shown as successful.
5. Only after the site works, update the GitHub About website field:
   ```powershell
   gh repo edit Sunan869/aiks --homepage "https://sunan869.github.io/aiks/"
   ```

The homepage remains a release-download link until Pages is enabled. The website does not collect analytics, does not run third-party scripts, and does not require an AIKS backend.
