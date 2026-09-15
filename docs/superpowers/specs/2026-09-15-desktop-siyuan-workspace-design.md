# AIKS Desktop × SiYuan 原生工作区设计

## 目标

在 AIKS Desktop 中提供完整的知识创建、编辑、收藏、归档和搜索入口，但不在 AIKS 内重复实现一套文档管理系统。优先复用随 AIKS 启动的 SiYuan 原生 Web 工作区，同时保留 AIKS 自己的 Session、KnowledgeItem、Pipeline、来源追踪、同步和混合搜索能力。

## 产品边界

### AIKS 继续负责

- CLI Session 发现、解析、清洗和 Pipeline。
- AI 知识提炼与 KnowledgeItem 本地状态。
- KnowledgeItem → SiYuan 同步、冲突检测和 baseline 语义。
- AIKS 自有全文/向量混合搜索，用于提炼知识和来源追踪。
- Desktop 的统一入口、状态提示和导航。

### SiYuan 原生工作区负责

- 手工创建笔记/文档。
- 富文本/块编辑。
- 收藏、归档、目录和标签等知识组织。
- SiYuan 全库原生搜索。
- 其他 SiYuan 已成熟提供的编辑器能力。

AIKS 不增加第二套收藏/归档字段，不直接修改 SiYuan SQLite 或 `.sy` 文件。

## Desktop 交互

知识库页面提供两个连续视图：

1. **AI 提炼**：保留现有 KnowledgeItem 列表、分类筛选、同步到 SiYuan。
2. **SiYuan 工作区**：直接嵌入当前 embedded SiYuan runtime 的 Web UI。用户在这里完成创建、编辑、收藏、归档、目录组织和 SiYuan 原生搜索。

SiYuan 工作区顶部仅保留 AIKS 薄层工具栏：状态、刷新、独立窗口兜底。独立窗口继续复用现有 `open_knowledge_window`，不是另一套业务实现。

搜索页面保留 AIKS 混合搜索，同时提供切换到 SiYuan 原生搜索工作区的入口，明确两种搜索的语义：AIKS 搜索用于提炼知识/来源追踪，SiYuan 搜索用于用户知识库全文检索。

## 可靠性与安全

- iframe 只接受 embedded runtime 返回的 loopback HTTP(S) URL（`127.0.0.1`、`localhost`、`::1`），拒绝任意远程 URL。
- Mock/普通浏览器开发模式不尝试加载本机 SiYuan，而显示说明占位。
- iframe 无法使用时，始终保留“独立窗口打开”兜底。
- 不通过 iframe 注入 Tauri IPC，不给 SiYuan 页面新增敏感权限。
- 不改变 KnowledgeItem 数据模型和现有冲突处理规则。

## 验收

- Desktop API 抽象提供 `getSiyuanUrl()` 和 `openSiyuanWorkspace()`，页面不直接散落 `invoke()`。
- 知识库可在 AI 提炼和 SiYuan 工作区间切换，无需离开 Desktop 主导航。
- SiYuan runtime 未就绪、URL 非 loopback、Mock 模式都有明确降级状态。
- AIKS 搜索行为保持不变，并能一键进入 SiYuan 原生搜索工作区。
- `npm test`、`npm run build`、workspace Rust CI 和 repository hygiene 全部通过。
