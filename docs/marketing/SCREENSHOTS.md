# AIKS product screenshots / 产品界面截图指南

The bilingual README files already contain a **commented-out** two-column, four-image gallery. Enable it only when authentic, privacy-reviewed captures are available. Do **not** publish AI-generated interface mockups as real product screenshots.

中英文 README 已准备好「两列四图」的隐藏模板。只有真实截图经过脱敏后，才取消注释展示；不要把生成的界面示意图冒充产品截图。

## Capture checklist / 截图清单

| File name | Chinese caption | English caption | Suggested content |
| --- | --- | --- | --- |
| `01-work-records.png` | 工作记录 | Work records | Imported sessions, source filters, preview; show realistic anonymized content |
| `02-data-sources.png` | 数据源管理 | Data sources | Available local providers and enable/disable controls |
| `03-knowledge-workspace.png` | 知识提炼 | Knowledge workspace | Structured knowledge item with categories, tags, and source trace |
| `04-search.png` | 检索与问答 | Search and Q&A | Full-text or optional semantic search with result snippets |

Save files in `docs/media/screenshots/`. PNG recommended, similar aspect ratio per row, 1600×900 or 1440×900 if convenient. Prefer a consistent light/dark theme and legible typography.

**Privacy requirements:** redact API keys, tokens, real chat contents, internal URLs/IP addresses, corporate project names, local user/home paths, email addresses, and other personal or internal data. A demo profile or sanitized test data is best. Avoid cropping away the entire navigation and context.

After the files exist, remove the outer `<!-- ... -->` comments enclosing the galleries in both `README.md` and `README.en.md`. Ensure each referenced path exists. Commit the four PNGs and the two updated READMEs in one commit.

## GitHub Social Preview / 仓库分享封面

- The checked-in `docs/assets/aiks-social-preview.svg` is the README hero visual (abstract branding, **not** a UI screenshot).
- Export a 1280×640 PNG for GitHub Social Preview. The PNG must be uploaded by a maintainer via repository **Settings → General → Social preview**; GitHub does not automatically set Social Preview from a committed README image.
- Avoid brand logos or product endorsement claims for upstream tools.
