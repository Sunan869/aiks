import { ExternalLink } from "lucide-react";

const THIRD_PARTY = [
  {
    name: "SiYuan",
    version: "3.8.3",
    license: "AGPL-3.0",
    url: "https://github.com/siyuan-note/siyuan",
    note: "Embedded knowledge base engine (bundled runtime)",
    highlighted: true,
  },
  {
    name: "AICoder Session Viewer",
    version: null,
    license: "MIT",
    url: "https://github.com/seastart/aicoder-session-viewer",
    note: "Session provider parser reference",
    highlighted: false,
  },
  {
    name: "CC Switch",
    version: null,
    license: "MIT",
    url: "https://github.com/farion1231/cc-switch",
    note: "Session scanning reference",
    highlighted: false,
  },
  {
    name: "ccusage",
    version: null,
    license: "MIT",
    url: "https://github.com/ccusage/ccusage",
    note: "Multi-agent source reference",
    highlighted: false,
  },
];

export default function AboutPage() {
  return (
    <div className="p-6 max-w-xl">
      <div className="mb-6">
        <div className="text-4xl font-bold text-blue-600 mb-1">AIKS</div>
        <div className="text-gray-400 text-sm">AI Knowledge Sync · v0.2.0</div>
        <div className="mt-3 text-sm text-gray-600 dark:text-gray-400">
          本地优先的 AI Session 自动知识沉淀工具。
          自动采集 Claude Code、Codex、Gemini CLI、OpenCode 的对话记录，
          同步到内置 SiYuan 知识库，支持搜索、双链、AI 问答。
        </div>
      </div>

      {/* SiYuan notice */}
      <div className="mb-4 p-3 bg-amber-50 dark:bg-amber-900/20 rounded-lg border border-amber-200 dark:border-amber-700 text-xs text-amber-700 dark:text-amber-300">
        AIKS 内置了 SiYuan 知识引擎（AGPL-3.0），并将其作为独立进程运行。
        SiYuan 源代码可在上方链接查阅。
      </div>

      <div className="mb-6">
        <div className="text-xs font-semibold text-gray-400 uppercase tracking-wider mb-3">
          内置/引用的开源组件
        </div>
        <div className="space-y-2">
          {THIRD_PARTY.map((item) => (
            <div
              key={item.name}
              className={`bg-white dark:bg-gray-800 rounded-lg border p-3 ${
                item.highlighted
                  ? "border-amber-300 dark:border-amber-600"
                  : "border-gray-200 dark:border-gray-700"
              }`}
            >
              <div className="flex items-center justify-between">
                <div className="font-medium text-sm">
                  {item.name}
                  {item.version && (
                    <span className="ml-2 text-xs text-gray-400">v{item.version}</span>
                  )}
                </div>
                <span className={`text-xs px-2 py-0.5 rounded ${
                  item.license === "AGPL-3.0"
                    ? "bg-amber-50 dark:bg-amber-900/30 text-amber-600 dark:text-amber-400"
                    : "bg-blue-50 dark:bg-blue-900/30 text-blue-600 dark:text-blue-400"
                }`}>
                  {item.license}
                </span>
              </div>
              <div className="text-xs text-gray-400 mt-0.5">{item.note}</div>
              <a
                href={item.url}
                target="_blank"
                rel="noreferrer"
                className="text-xs text-blue-500 hover:text-blue-600 flex items-center gap-1 mt-1"
              >
                <ExternalLink className="w-3 h-3" />
                {item.url.replace("https://github.com/", "")}
              </a>
            </div>
          ))}
        </div>
      </div>

      <div className="text-xs text-gray-400 border-t border-gray-200 dark:border-gray-700 pt-4 space-y-1">
        <p>SiYuan 以 AGPL-3.0 授权，随 AIKS 内嵌发布。</p>
        <p>源代码：https://github.com/siyuan-note/siyuan</p>
        <p>本 AIKS 发行版中包含 SiYuan v3.8.3 的可执行文件及运行资源。</p>
      </div>
    </div>
  );
}
