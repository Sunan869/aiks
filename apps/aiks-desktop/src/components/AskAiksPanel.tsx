import { useEffect, useRef, useState } from "react";
import {
  Bot,
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
      behavior: "smooth",
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
        className="absolute bottom-7 right-0 top-12 flex w-[min(440px,calc(100vw-24px))] flex-col border-l border-gray-200 bg-white shadow-2xl dark:border-gray-700 dark:bg-gray-900"
        onMouseDown={event => event.stopPropagation()}
        aria-label="问 AIKS"
      >
        <header className="flex h-14 flex-shrink-0 items-center gap-3 border-b border-gray-200 px-4 dark:border-gray-700">
          <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-blue-50 text-blue-600 dark:bg-blue-950/40 dark:text-blue-300">
            <Sparkles className="h-4 w-4" />
          </div>
          <div className="min-w-0 flex-1">
            <div className="text-sm font-semibold text-gray-900 dark:text-gray-100">问 AIKS</div>
            <div className="truncate text-[11px] text-gray-400">基于知识与 AI 对话记录回答 · 带引用</div>
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

        <div ref={scrollRef} className="min-h-0 flex-1 overflow-y-auto px-4 py-4">
          {messages.length === 0 ? (
            <div className="flex h-full min-h-[260px] flex-col items-center justify-center text-center">
              <div className="flex h-12 w-12 items-center justify-center rounded-2xl bg-blue-50 text-blue-600 dark:bg-blue-950/40 dark:text-blue-300">
                <Bot className="h-6 w-6" />
              </div>
              <div className="mt-4 text-sm font-medium text-gray-700 dark:text-gray-200">
                直接问你的 AIKS 知识库
              </div>
              <div className="mt-2 max-w-xs text-xs leading-5 text-gray-400">
                例如：“我们之前 KingBase 迁移遇到过哪些问题？”、“总结一下最近讨论的向量模型部署方案。”
              </div>
            </div>
          ) : (
            <div className="space-y-5">
              {messages.map(message => (
                <Message
                  key={message.id}
                  message={message}
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

        <div className="flex-shrink-0 border-t border-gray-200 p-3 dark:border-gray-700">
          <div className="rounded-xl border border-gray-200 bg-gray-50 p-2 focus-within:border-blue-300 focus-within:bg-white dark:border-gray-700 dark:bg-gray-800 dark:focus-within:border-blue-700 dark:focus-within:bg-gray-850">
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
              rows={3}
              placeholder="问问 AIKS…"
              className="w-full resize-none bg-transparent px-1 text-sm leading-5 text-gray-900 outline-none placeholder:text-gray-400 disabled:opacity-60 dark:text-gray-100"
            />
            <div className="mt-1 flex items-center justify-between px-1">
              <span className="text-[10px] text-gray-400">Enter 发送 · Shift+Enter 换行</span>
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
          <div className="mt-2 text-center text-[10px] text-gray-400">
            AI 仅根据检索到的 AIKS 内容回答，重要结论请查看引用来源
          </div>
        </div>
      </aside>
    </div>
  );
}

function Message({
  message,
  onOpenCitation,
}: {
  message: UiMessage;
  onOpenCitation: (citation: RagCitation) => void;
}) {
  if (message.role === "user") {
    return (
      <div className="flex justify-end">
        <div className="max-w-[86%] rounded-2xl rounded-br-md bg-blue-600 px-3.5 py-2.5 text-sm leading-6 text-white">
          {message.content}
        </div>
      </div>
    );
  }

  return (
    <div className="flex items-start gap-2.5">
      <div className="mt-0.5 flex h-7 w-7 flex-shrink-0 items-center justify-center rounded-lg bg-blue-50 text-blue-600 dark:bg-blue-950/40 dark:text-blue-300">
        <Bot className="h-3.5 w-3.5" />
      </div>
      <div className="min-w-0 flex-1">
        {message.content ? (
          <div className="whitespace-pre-wrap text-sm leading-6 text-gray-800 dark:text-gray-100">
            {message.content}
          </div>
        ) : (
          <div className="rounded-xl bg-gray-50 px-3 py-2.5 text-xs text-gray-500 dark:bg-gray-800 dark:text-gray-300">
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
          <div className="mt-3 space-y-1.5">
            <div className="text-[11px] font-medium text-gray-400">引用来源</div>
            {message.citations.map(citation => (
              <button
                key={`${citation.corpus}:${citation.entityId}:${citation.chunkId ?? citation.index}`}
                type="button"
                onClick={() => onOpenCitation(citation)}
                className="block w-full rounded-lg border border-gray-200 bg-gray-50 px-2.5 py-2 text-left transition hover:border-blue-200 hover:bg-blue-50/50 dark:border-gray-700 dark:bg-gray-800 dark:hover:border-blue-900 dark:hover:bg-blue-950/20"
              >
                <div className="flex items-center gap-2">
                  <span className="flex h-5 min-w-5 items-center justify-center rounded bg-gray-200 px-1 text-[10px] font-semibold text-gray-600 dark:bg-gray-700 dark:text-gray-200">
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
                  <div className="mt-1.5 line-clamp-2 pl-7 text-[11px] leading-4 text-gray-400">
                    {citation.snippet}
                  </div>
                )}
              </button>
            ))}
          </div>
        )}

        {message.model && (
          <div className="mt-2 text-[10px] text-gray-300 dark:text-gray-600">
            {message.model}
          </div>
        )}
      </div>
    </div>
  );
}
