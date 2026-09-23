import { useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import {
  Bot,
  BookOpen,
  Copy,
  FileText,
  Loader2,
  MessageSquareText,
  Send,
  Sparkles,
  Trash2,
  X,
} from "lucide-react";
import { askAiksProgressively } from "../api/rag-progress";
import type { RagCitation, RagTurn } from "../api/rag";

interface Props {
  open: boolean;
  aiHealthy: boolean;
  onClose: () => void;
  onOpenCitation: (citation: RagCitation) => void;
}

type UiMessage = {
  id: string;
  role: "user" | "assistant";
  content: string;
  citations?: RagCitation[];
  model?: string;
  warnings?: string[];
};

const QUICK_PROMPTS = [
  "总结一下最近讨论的向量模型部署方案",
  "我们之前遇到过哪些数据库迁移问题？",
  "帮我从知识库里找一下最近的关键技术结论",
];

function messageId(): string {
  return `${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

export default function AskAiksPanel({
  open,
  aiHealthy,
  onClose,
  onOpenCitation,
}: Props) {
  const [messages, setMessages] = useState<UiMessage[]>([]);
  const [input, setInput] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const timer = window.setTimeout(() => textareaRef.current?.focus(), 50);
    return () => window.clearTimeout(timer);
  }, [open]);

  useEffect(() => {
    if (!open) return;
    scrollRef.current?.scrollTo({
      top: scrollRef.current.scrollHeight,
      behavior: loading ? "auto" : "smooth",
    });
  }, [open, messages, loading]);

  useEffect(() => {
    if (!open) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [open, onClose]);

  const clear = () => {
    if (loading) return;
    setMessages([]);
    setInput("");
    setError(null);
    window.setTimeout(() => textareaRef.current?.focus(), 0);
  };

  const submit = async () => {
    const question = input.trim();
    if (!question || loading || !aiHealthy) return;

    const history: RagTurn[] = messages.map(message => ({
      role: message.role,
      content: message.content,
    }));
    const assistantId = messageId();

    setMessages(current => [
      ...current,
      { id: messageId(), role: "user", content: question },
      { id: assistantId, role: "assistant", content: "" },
    ]);
    setInput("");
    setError(null);
    setLoading(true);

    try {
      const result = await askAiksProgressively(
        { question, history },
        {
          onDelta: delta => {
            setMessages(current => current.map(message => (
              message.id === assistantId
                ? { ...message, content: message.content + delta }
                : message
            )));
          },
        },
      );
      setMessages(current => current.map(message => (
        message.id === assistantId
          ? {
              ...message,
              content: result.answer || message.content,
              citations: result.citations,
              model: result.model,
              warnings: result.warnings,
            }
          : message
      )));
    } catch (reason) {
      setMessages(current => current.filter(
        message => message.id !== assistantId || message.content.trim().length > 0,
      ));
      setError(String(reason));
    } finally {
      setLoading(false);
      window.setTimeout(() => textareaRef.current?.focus(), 0);
    }
  };

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-[80] bg-black/15 backdrop-blur-[1px]" onMouseDown={onClose}>
      <aside
        className="absolute bottom-7 right-0 top-12 flex w-[min(680px,calc(100vw-16px))] flex-col border-l border-gray-200 bg-white shadow-2xl dark:border-gray-700 dark:bg-gray-900"
        onMouseDown={event => event.stopPropagation()}
        aria-label="问 AIKS"
      >
        <header className="flex h-16 flex-shrink-0 items-center gap-3 border-b border-gray-200 px-5 dark:border-gray-700">
          <div className="flex h-9 w-9 items-center justify-center rounded-xl bg-blue-600 text-white shadow-sm shadow-blue-200 dark:shadow-none">
            <Sparkles className="h-4 w-4" />
          </div>
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-2">
              <div className="text-[15px] font-semibold text-gray-900 dark:text-gray-100">问 AIKS</div>
              <span className="rounded-full bg-blue-50 px-2 py-0.5 text-[10px] font-medium text-blue-600 dark:bg-blue-950/40 dark:text-blue-300">知识库问答</span>
            </div>
            <div className="mt-0.5 truncate text-[11px] text-gray-400">基于知识与 AI 对话记录检索回答，并提供可追溯引用</div>
          </div>
          <button
            type="button"
            onClick={clear}
            disabled={loading || messages.length === 0}
            className="rounded-md p-1.5 text-gray-400 hover:bg-gray-100 hover:text-gray-700 disabled:opacity-30 dark:hover:bg-gray-800 dark:hover:text-gray-200"
            title="新对话"
          >
            <Trash2 className="h-4 w-4" />
          </button>
          <button
            type="button"
            onClick={onClose}
            className="rounded-md p-1.5 text-gray-400 hover:bg-gray-100 hover:text-gray-700 dark:hover:bg-gray-800 dark:hover:text-gray-200"
            title="关闭"
          >
            <X className="h-4 w-4" />
          </button>
        </header>

        <div ref={scrollRef} className="min-h-0 flex-1 overflow-y-auto bg-gray-50/60 px-5 py-5 dark:bg-gray-950/20">
          {messages.length === 0 ? (
            <div className="flex h-full min-h-[320px] w-full flex-col justify-center">
              <div className="flex items-center gap-3">
                <div className="flex h-11 w-11 items-center justify-center rounded-2xl bg-blue-600 text-white shadow-sm shadow-blue-200 dark:shadow-none">
                  <Sparkles className="h-5 w-5" />
                </div>
                <div>
                  <div className="text-base font-semibold text-gray-900 dark:text-gray-100">有什么想从 AIKS 里找的？</div>
                  <div className="mt-1 text-xs text-gray-400">我会先检索知识与历史 AI 对话，再基于证据回答。</div>
                </div>
              </div>
              <div className="mt-6 grid gap-2">
                {QUICK_PROMPTS.map(prompt => (
                  <button
                    key={prompt}
                    type="button"
                    onClick={() => {
                      setInput(prompt);
                      window.setTimeout(() => textareaRef.current?.focus(), 0);
                    }}
                    className="group flex items-center gap-3 rounded-xl border border-gray-200 bg-white px-4 py-3 text-left text-sm text-gray-600 shadow-sm transition hover:border-blue-200 hover:bg-blue-50/40 hover:text-gray-900 dark:border-gray-700 dark:bg-gray-900 dark:text-gray-300 dark:hover:border-blue-900 dark:hover:bg-blue-950/20"
                  >
                    <BookOpen className="h-4 w-4 flex-shrink-0 text-gray-400 transition group-hover:text-blue-500" />
                    <span>{prompt}</span>
                  </button>
                ))}
              </div>
            </div>
          ) : (
            <div className="w-full space-y-7">
              {messages.map((message, index) => (
                <Message
                  key={message.id}
                  message={message}
                  streaming={loading && message.role === "assistant" && index === messages.length - 1}
                  onOpenCitation={onOpenCitation}
                />
              ))}

            </div>
          )}
        </div>

        {!aiHealthy && (
          <div className="mx-4 mb-2 rounded-lg border border-amber-200 bg-amber-50 px-3 py-2 text-xs text-amber-700 dark:border-amber-900 dark:bg-amber-950/30 dark:text-amber-200">
            AI 服务当前不可用，请先在设置中检查模型配置。
          </div>
        )}

        {error && (
          <div className="mx-4 mb-2 rounded-lg border border-red-200 bg-red-50 px-3 py-2 text-xs text-red-600 dark:border-red-900 dark:bg-red-950/30 dark:text-red-300">
            {error}
          </div>
        )}

        <div className="flex-shrink-0 border-t border-gray-200 bg-white px-5 pb-4 pt-3 dark:border-gray-700 dark:bg-gray-900">
          <div className="w-full rounded-2xl border border-gray-200 bg-white p-2.5 shadow-[0_6px_24px_rgba(15,23,42,0.06)] transition focus-within:border-blue-300 focus-within:shadow-[0_8px_28px_rgba(37,99,235,0.10)] dark:border-gray-700 dark:bg-gray-800">
            <textarea
              ref={textareaRef}
              value={input}
              onChange={event => setInput(event.target.value)}
              onKeyDown={event => {
                if (event.key === "Enter" && !event.shiftKey) {
                  event.preventDefault();
                  void submit();
                }
              }}
              disabled={loading}
              rows={2}
              placeholder="向 AIKS 提问…"
              className="w-full resize-none bg-transparent px-1.5 py-1 text-sm leading-6 text-gray-900 outline-none placeholder:text-gray-400 disabled:opacity-60 dark:text-gray-100"
            />
            <div className="mt-1.5 flex items-center justify-between px-1">
              <span className="text-[10px] text-gray-400">Enter 发送 · Shift+Enter 换行 · 基于检索证据回答</span>
              <button
                type="button"
                onClick={() => void submit()}
                disabled={loading || !aiHealthy || !input.trim()}
                className="flex h-8 w-8 items-center justify-center rounded-lg bg-blue-600 text-white transition hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-35"
                title="发送"
              >
                {loading ? <Loader2 className="h-4 w-4 animate-spin" /> : <Send className="h-4 w-4" />}
              </button>
            </div>
          </div>
          <div className="mt-2 w-full text-center text-[10px] text-gray-400">
            AIKS 可能生成不准确内容，重要结论请结合引用来源核验
          </div>
        </div>
      </aside>
    </div>
  );
}


function renderInline(
  text: string,
  citations: RagCitation[] | undefined,
  onOpenCitation: (citation: RagCitation) => void,
): ReactNode[] {
  const pattern = /(\*\*[^*]+\*\*|\x60[^\x60]+\x60|\[\d+\])/g;
  const parts: ReactNode[] = [];
  let cursor = 0;

  for (const match of text.matchAll(pattern)) {
    const index = match.index ?? 0;
    if (index > cursor) parts.push(text.slice(cursor, index));

    const token = match[0];
    const citationMatch = /^\[(\d+)\]$/.exec(token);
    if (citationMatch) {
      const citation = citations?.find(item => item.index === Number(citationMatch[1]));
      parts.push(citation ? (
        <button
          key={"citation-" + index}
          type="button"
          onClick={() => onOpenCitation(citation)}
          className="mx-0.5 inline-flex min-w-5 items-center justify-center rounded-md bg-blue-50 px-1.5 py-0.5 text-[10px] font-semibold leading-4 text-blue-600 transition hover:bg-blue-100 dark:bg-blue-950/40 dark:text-blue-300 dark:hover:bg-blue-900/50"
          title={citation.title}
        >
          {citation.index}
        </button>
      ) : token);
    } else if (token.startsWith("**")) {
      parts.push(
        <strong key={"strong-" + index} className="font-semibold text-gray-950 dark:text-white">
          {token.slice(2, -2)}
        </strong>,
      );
    } else {
      parts.push(
        <code
          key={"code-" + index}
          className="rounded bg-gray-100 px-1.5 py-0.5 font-mono text-[0.9em] text-pink-600 dark:bg-gray-800 dark:text-pink-300"
        >
          {token.slice(1, -1)}
        </code>,
      );
    }
    cursor = index + token.length;
  }

  if (cursor < text.length) parts.push(text.slice(cursor));
  return parts;
}

function splitTableRow(value: string): string[] {
  return value
    .trim()
    .replace(/^\|/, "")
    .replace(/\|$/, "")
    .split("|")
    .map(cell => cell.trim());
}

function isTableSeparator(value: string): boolean {
  const cells = splitTableRow(value);
  return cells.length > 0 && cells.every(cell => /^:?-{3,}:?$/.test(cell));
}

function MarkdownAnswer({
  content,
  citations,
  onOpenCitation,
  streaming,
}: {
  content: string;
  citations?: RagCitation[];
  onOpenCitation: (citation: RagCitation) => void;
  streaming: boolean;
}) {
  const fence = String.fromCharCode(96, 96, 96);
  const lines = content.split("\n");
  const nodes: ReactNode[] = [];
  let index = 0;

  while (index < lines.length) {
    const trimmed = lines[index].trim();
    if (!trimmed) {
      index += 1;
      continue;
    }

    if (trimmed.startsWith(fence)) {
      const language = trimmed.slice(3).trim();
      const codeLines: string[] = [];
      index += 1;
      while (index < lines.length && !lines[index].trim().startsWith(fence)) {
        codeLines.push(lines[index]);
        index += 1;
      }
      if (index < lines.length) index += 1;
      nodes.push(
        <div
          key={"code-block-" + index}
          className="my-4 overflow-hidden rounded-xl border border-gray-200 bg-gray-950 dark:border-gray-700"
        >
          {language && (
            <div className="border-b border-white/10 px-3 py-1.5 text-[10px] font-medium uppercase tracking-wide text-gray-400">
              {language}
            </div>
          )}
          <pre className="overflow-x-auto p-3 text-xs leading-5 text-gray-100">
            <code>{codeLines.join("\n")}</code>
          </pre>
        </div>,
      );
      continue;
    }

    if (
      index + 1 < lines.length
      && trimmed.includes("|")
      && isTableSeparator(lines[index + 1])
    ) {
      const headers = splitTableRow(lines[index]);
      const rows: string[][] = [];
      index += 2;
      while (index < lines.length && lines[index].trim() && lines[index].includes("|")) {
        rows.push(splitTableRow(lines[index]));
        index += 1;
      }
      nodes.push(
        <div key={"table-" + index} className="my-4 overflow-x-auto rounded-xl border border-gray-200 dark:border-gray-700">
          <table className="min-w-full border-collapse bg-white text-left text-xs dark:bg-gray-900">
            <thead className="bg-gray-50 dark:bg-gray-800">
              <tr>
                {headers.map((header, cellIndex) => (
                  <th
                    key={cellIndex}
                    className="border-b border-gray-200 px-3 py-2 font-semibold text-gray-700 dark:border-gray-700 dark:text-gray-200"
                  >
                    {renderInline(header, citations, onOpenCitation)}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {rows.map((row, rowIndex) => (
                <tr key={rowIndex} className="border-b border-gray-100 last:border-0 dark:border-gray-800">
                  {headers.map((_, cellIndex) => (
                    <td key={cellIndex} className="px-3 py-2 align-top leading-5 text-gray-600 dark:text-gray-300">
                      {renderInline(row[cellIndex] ?? "", citations, onOpenCitation)}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>,
      );
      continue;
    }

    const heading = /^(#{1,4})\s+(.*)$/.exec(trimmed);
    if (heading) {
      const level = heading[1].length;
      nodes.push(
        <div
          key={"heading-" + index}
          className={level <= 2
            ? "mb-2 mt-5 text-[15px] font-semibold leading-6 text-gray-950 dark:text-white"
            : "mb-1.5 mt-4 text-sm font-semibold leading-6 text-gray-900 dark:text-gray-100"}
        >
          {renderInline(heading[2], citations, onOpenCitation)}
        </div>,
      );
      index += 1;
      continue;
    }

    if (/^[-*]\s+/.test(trimmed)) {
      const items: string[] = [];
      while (index < lines.length && /^\s*[-*]\s+/.test(lines[index])) {
        items.push(lines[index].replace(/^\s*[-*]\s+/, ""));
        index += 1;
      }
      nodes.push(
        <ul key={"ul-" + index} className="my-2.5 space-y-1.5 pl-5 text-sm leading-6 text-gray-700 dark:text-gray-200">
          {items.map((item, itemIndex) => (
            <li key={itemIndex} className="list-disc pl-1 marker:text-gray-400">
              {renderInline(item, citations, onOpenCitation)}
            </li>
          ))}
        </ul>,
      );
      continue;
    }

    if (/^\d+\.\s+/.test(trimmed)) {
      const items: string[] = [];
      while (index < lines.length && /^\s*\d+\.\s+/.test(lines[index])) {
        items.push(lines[index].replace(/^\s*\d+\.\s+/, ""));
        index += 1;
      }
      nodes.push(
        <ol key={"ol-" + index} className="my-2.5 list-decimal space-y-1.5 pl-5 text-sm leading-6 text-gray-700 dark:text-gray-200">
          {items.map((item, itemIndex) => (
            <li key={itemIndex} className="pl-1 marker:font-medium marker:text-gray-500">
              {renderInline(item, citations, onOpenCitation)}
            </li>
          ))}
        </ol>,
      );
      continue;
    }

    if (trimmed === "---") {
      nodes.push(<hr key={"hr-" + index} className="my-4 border-gray-200 dark:border-gray-700" />);
      index += 1;
      continue;
    }

    const paragraph: string[] = [trimmed];
    index += 1;
    while (
      index < lines.length
      && lines[index].trim()
      && !/^(#{1,4})\s+/.test(lines[index].trim())
      && !/^\s*[-*]\s+/.test(lines[index])
      && !/^\s*\d+\.\s+/.test(lines[index])
      && !lines[index].trim().startsWith(fence)
      && lines[index].trim() !== "---"
    ) {
      paragraph.push(lines[index].trim());
      index += 1;
    }
    nodes.push(
      <p key={"p-" + index} className="my-2 text-sm leading-6 text-gray-700 dark:text-gray-200">
        {renderInline(paragraph.join(" "), citations, onOpenCitation)}
      </p>,
    );
  }

  return (
    <div>
      {nodes}
      {streaming && (
        <span
          className="ml-0.5 inline-block h-4 w-1 animate-pulse rounded-full bg-blue-500 align-[-2px]"
          aria-label="正在生成"
        />
      )}
    </div>
  );
}

function Message({
  message,
  streaming,
  onOpenCitation,
}: {
  message: UiMessage;
  streaming: boolean;
  onOpenCitation: (citation: RagCitation) => void;
}) {
  if (message.role === "user") {
    return (
      <div className="flex justify-end">
        <div className="max-w-[78%] rounded-2xl rounded-br-md bg-blue-600 px-4 py-2.5 text-sm leading-6 text-white shadow-sm">
          {message.content}
        </div>
      </div>
    );
  }

  return (
    <div className="flex items-start gap-3">
      <div className="mt-0.5 flex h-8 w-8 flex-shrink-0 items-center justify-center rounded-xl border border-blue-100 bg-blue-50 text-blue-600 dark:border-blue-900/60 dark:bg-blue-950/40 dark:text-blue-300">
        <Bot className="h-4 w-4" />
      </div>
      <div className="min-w-0 flex-1 pt-0.5">
        {message.content ? (
          <MarkdownAnswer
            content={message.content}
            citations={message.citations}
            onOpenCitation={onOpenCitation}
            streaming={streaming}
          />
        ) : (
          <div className="rounded-xl border border-gray-100 bg-white px-3 py-2.5 text-xs text-gray-500 shadow-sm dark:border-gray-800 dark:bg-gray-900 dark:text-gray-300">
            <div className="flex items-center gap-2">
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
              <span>正在检索知识并组织回答…</span>
            </div>
          </div>
        )}

        {message.warnings && message.warnings.length > 0 && (
          <div className="mt-2 rounded-md bg-amber-50 px-2.5 py-2 text-[11px] text-amber-700 dark:bg-amber-950/30 dark:text-amber-200">
            {message.warnings[0]}
          </div>
        )}

        {message.citations && message.citations.length > 0 && (
          <details className="group mt-4 overflow-hidden rounded-xl border border-gray-200 bg-white dark:border-gray-700 dark:bg-gray-900">
            <summary className="flex cursor-pointer list-none items-center gap-2 px-3 py-2.5 text-xs font-medium text-gray-600 transition hover:bg-gray-50 dark:text-gray-300 dark:hover:bg-gray-800">
              <BookOpen className="h-3.5 w-3.5 text-blue-500" />
              <span>参考来源</span>
              <span className="rounded-full bg-gray-100 px-1.5 py-0.5 text-[10px] text-gray-500 dark:bg-gray-800 dark:text-gray-400">
                {message.citations.length}
              </span>
              <span className="ml-auto text-[10px] font-normal text-gray-400 group-open:hidden">展开</span>
              <span className="ml-auto hidden text-[10px] font-normal text-gray-400 group-open:inline">收起</span>
            </summary>
            <div className="space-y-1.5 border-t border-gray-100 p-2 dark:border-gray-800">
              {message.citations.map(citation => (
                <button
                  key={citation.corpus + ":" + citation.entityId + ":" + (citation.chunkId ?? citation.index)}
                  type="button"
                  onClick={() => onOpenCitation(citation)}
                  className="block w-full rounded-lg px-2.5 py-2 text-left transition hover:bg-blue-50/60 dark:hover:bg-blue-950/20"
                >
                  <div className="flex items-center gap-2">
                    <span className="flex h-5 min-w-5 items-center justify-center rounded-md bg-blue-50 px-1 text-[10px] font-semibold text-blue-600 dark:bg-blue-950/50 dark:text-blue-300">
                      {citation.index}
                    </span>
                    {citation.corpus === "knowledge"
                      ? <FileText className="h-3.5 w-3.5 flex-shrink-0 text-blue-500" />
                      : <MessageSquareText className="h-3.5 w-3.5 flex-shrink-0 text-violet-500" />}
                    <span className="min-w-0 flex-1 truncate text-xs font-medium text-gray-700 dark:text-gray-200">
                      {citation.title}
                    </span>
                  </div>
                  {citation.snippet && (
                    <div className="mt-1 line-clamp-2 pl-7 text-[11px] leading-4 text-gray-400">
                      {citation.snippet}
                    </div>
                  )}
                </button>
              ))}
            </div>
          </details>
        )}

        {message.content && (
          <div className="mt-2.5 flex items-center gap-1 text-[10px] text-gray-400">
            <button
              type="button"
              onClick={() => void navigator.clipboard?.writeText(message.content)}
              className="inline-flex items-center gap-1 rounded-md px-1.5 py-1 transition hover:bg-gray-100 hover:text-gray-600 dark:hover:bg-gray-800 dark:hover:text-gray-300"
              title="复制回答"
            >
              <Copy className="h-3 w-3" />
              复制
            </button>
            {message.model && (
              <span className="ml-auto text-gray-300 dark:text-gray-600">{message.model}</span>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
