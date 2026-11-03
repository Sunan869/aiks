# AIKS product screenshots / 产品界面截图指南

The bilingual README files show a **two-column, four-image gallery** with real AIKS desktop screenshots. Do **not** publish AI-generated interface mockups as real product screenshots.

中英文 README 已启用真实截图的「两列四图」展示。更新图片前请检查隐私信息；不要把生成的界面示意图冒充产品截图。

## Capture checklist / 截图清单

| File name | Chinese caption | English caption | Suggested content |
| --- | --- | --- | --- |
| `01-data-sources.png` | 数据源管理 | Data sources | Available local providers, source state and sync actions |
| `02-processing-center.png` | 处理中心 | Processing center | Session count, AI extraction status, queue and task list |
| `03-knowledge-workspace.png` | 知识库 | Knowledge base | Structured knowledge item and reference to original session |
| `04-ask-aiks.png` | 问 AIKS | Ask AIKS | Knowledge-grounded Q&A and source context |

Save files in `docs/media/screenshots/`. PNG recommended, similar aspect ratio per row, 1600×900 or 1440×900 if convenient. Prefer a consistent light/dark theme and legible typography.

**Privacy requirements:** redact API keys, tokens, real chat contents, internal URLs/IP addresses, corporate project names, local user/home paths, email addresses, and other personal or internal data. A demo profile or sanitized test data is best. Avoid cropping away the entire navigation and context.

Both README galleries are enabled. When replacing screenshots, keep filenames and captions aligned in `README.md`, `README.en.md` and this document. Confirm every referenced path exists before committing.

## GitHub Social Preview / 仓库分享封面

- The checked-in `docs/assets/aiks-social-preview.svg` is the README hero visual (abstract branding, **not** a UI screenshot).
- Export a 1280×640 PNG for GitHub Social Preview. The PNG must be uploaded by a maintainer via repository **Settings → General → Social preview**; GitHub does not automatically set Social Preview from a committed README image.
- Avoid brand logos or product endorsement claims for upstream tools.
