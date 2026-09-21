/*
 * ChatGPT public share parser.
 * Adapted from evanhu1/chatgpt-share-parser (MIT), itself ported from ChatPeek.
 * Upstream: https://github.com/evanhu1/chatgpt-share-parser
 * See THIRD_PARTY_NOTICES.md.
 */

// Ported from ChatPeek: https://github.com/vl3c/ChatPeek

export type JsonScalar = string | number | boolean | null;
export type JsonValue = JsonScalar | JsonObject | JsonValue[];
export type JsonObject = { [key: string]: JsonValue };

export type ChatGptShareAsset = {
  assetType: "image" | "file";
  url: string;
  filename: string;
  description: string | null;
  downloadable: boolean;
};

export type ChatGptShareReplyType = "user" | "assistant" | "tool";

export type ChatGptShareReply = {
  authorName: string;
  type: ChatGptShareReplyType;
  statement: string;
  createdAt: number | null;
  assets: ChatGptShareAsset[];
};

export type ChatGptShareConversation = {
  shareId: string;
  aiModel: string;
  title: string;
  updatedAt: number | null;
  replies: ChatGptShareReply[];
};

export const CHATGPT_SHARE_HEADERS = {
  "User-Agent":
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/118.0.0.0 Safari/537.36",
  "Sec-Ch-Ua": "\"Chromium\";v=\"118\", \"Not=A?Brand\";v=\"24\"",
  "Sec-Ch-Ua-Mobile": "?0",
  "Sec-Ch-Ua-Platform": "\"Windows\"",
  Accept:
    "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8",
  "Accept-Language": "en-US,en;q=0.9",
  Referer: "https://chatgpt.com/",
} as const;

const PRIVATE_USE_PATTERN = /[\uE000-\uF8FF]/g;
const CITATION_TOKEN_PATTERN =
  /\s*(?:citeturn|navlist|turn\d+\w*)[^,\s]*,?/g;

export class ChatGptShareAccessError extends Error {}

export class ChatGptShareFetchError extends Error {
  status: number;

  constructor(message: string, status: number) {
    super(message);
    this.status = status;
  }
}

export class ChatGptShareParseError extends Error {}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function getString(value: unknown): string | null {
  return typeof value === "string" ? value : null;
}

function getNumber(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

export function getChatGptShareId(input: string | URL): string | null {
  let url: URL;

  try {
    url = typeof input === "string" ? new URL(input) : input;
  } catch {
    return null;
  }

  if (!["chatgpt.com", "chat.openai.com"].includes(url.hostname)) {
    return null;
  }

  const segments = url.pathname.split("/").filter(Boolean);
  if (segments[0] !== "share") {
    return null;
  }

  return segments[1] === "e"
    ? segments[2] ?? null
    : segments[1] ?? null;
}

export function isChatGptShareUrl(input: string | URL): boolean {
  return getChatGptShareId(input) !== null;
}

function extractScripts(html: string): Array<{
  attrs: Record<string, string>;
  text: string;
}> {
  const scripts: Array<{ attrs: Record<string, string>; text: string }> = [];
  const scriptPattern = /<script\b([^>]*)>([\s\S]*?)<\/script>/gi;

  for (const match of html.matchAll(scriptPattern)) {
    const attrs: Record<string, string> = {};
    const rawAttrs = match[1] ?? "";
    const attrPattern = /([^\s=]+)(?:\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s"'>]+)))?/g;

    for (const attrMatch of rawAttrs.matchAll(attrPattern)) {
      attrs[attrMatch[1].toLowerCase()] =
        attrMatch[2] ?? attrMatch[3] ?? attrMatch[4] ?? "";
    }

    scripts.push({ attrs, text: match[2] ?? "" });
  }

  return scripts;
}

function findCallArgument(text: string, startIndex: number) {
  let quote: string | null = null;
  let escaped = false;
  let depth = 1;

  for (let index = startIndex; index < text.length; index += 1) {
    const char = text[index];

    if (quote) {
      if (escaped) {
        escaped = false;
      } else if (char === "\\") {
        escaped = true;
      } else if (char === quote) {
        quote = null;
      }
      continue;
    }

    if (char === "\"" || char === "'" || char === "`") {
      quote = char;
      continue;
    }

    if (char === "(") {
      depth += 1;
      continue;
    }

    if (char === ")") {
      depth -= 1;
      if (depth === 0) {
        return {
          argument: text.slice(startIndex, index).trim(),
          endIndex: index + 1,
        };
      }
    }
  }

  return null;
}

function stripOuterParens(value: string): string {
  let result = value.trim();

  while (result.startsWith("(") && result.endsWith(")")) {
    result = result.slice(1, -1).trim();
  }

  return result;
}

function parseEnqueuedLoader(argument: string): unknown[] | null {
  const stripped = stripOuterParens(argument);
  let chunk: unknown = stripped;

  if (stripped.startsWith("\"")) {
    try {
      chunk = JSON.parse(stripped) as unknown;
    } catch {
      return null;
    }
  }

  if (typeof chunk === "string") {
    const trimmed = chunk.trim();
    if (!trimmed.startsWith("[")) {
      return null;
    }

    try {
      const parsed = JSON.parse(trimmed) as unknown;
      return Array.isArray(parsed) ? parsed : null;
    } catch {
      return null;
    }
  }

  if (Array.isArray(chunk)) {
    return chunk;
  }

  if (stripped.startsWith("[")) {
    try {
      const parsed = JSON.parse(stripped) as unknown;
      return Array.isArray(parsed) ? parsed : null;
    } catch {
      return null;
    }
  }

  return null;
}

export function extractLoaderPayload(html: string): unknown[] | null {
  for (const { text } of extractScripts(html)) {
    if (!text || !text.includes("streamController.enqueue")) {
      continue;
    }

    let start = 0;
    const call = "streamController.enqueue(";

    while (start < text.length) {
      const anchor = text.indexOf(call, start);
      if (anchor === -1) {
        break;
      }

      const argumentStart = anchor + call.length;
      const callArgument = findCallArgument(text, argumentStart);
      if (!callArgument) {
        break;
      }

      const payload = parseEnqueuedLoader(callArgument.argument);
      if (payload) {
        return payload;
      }

      start = callArgument.endIndex;
    }
  }

  return null;
}

export function decodeLoader(loader: unknown[]): Record<string, unknown> {
  const cache = new Map<number, unknown>();

  function decodeKey(rawKey: string): string {
    if (/^_\d+$/.test(rawKey)) {
      const index = Number(rawKey.slice(1));
      const candidate = loader[index];
      if (typeof candidate === "string") {
        return candidate;
      }
    }

    return rawKey;
  }

  function resolve(value: unknown): unknown {
    if (typeof value === "number" && Number.isInteger(value)) {
      if (cache.has(value)) {
        return cache.get(value);
      }

      if (value < 0 || value >= loader.length) {
        return value;
      }

      cache.set(value, null);
      const resolved = resolve(loader[value]);
      cache.set(value, resolved);
      return resolved;
    }

    if (Array.isArray(value)) {
      return value.map((item) => resolve(item));
    }

    if (isRecord(value)) {
      return Object.fromEntries(
        Object.entries(value).map(([key, item]) => [
          decodeKey(key),
          resolve(item),
        ]),
      );
    }

    return value;
  }

  const decoded: Record<string, unknown> = {};

  for (let index = 1; index < loader.length - 1; index += 2) {
    const key = loader[index];
    if (typeof key === "string" && !(key in decoded)) {
      decoded[key] = resolve(loader[index + 1]);
    }
  }

  return decoded;
}

function authorNameForRole(role: string | null): string {
  if (role === "user") {
    return "User";
  }

  if (role === "tool") {
    return "Tool";
  }

  return "Assistant";
}

export function stripPrivateUse(text: string): string {
  return text.replace(PRIVATE_USE_PATTERN, "");
}

export function stripCitationTokens(text: string): string {
  return text
    .split("\n")
    .map((line) => line.replace(CITATION_TOKEN_PATTERN, "").trimEnd())
    .join("\n");
}

export function summarizeToolPayload(data: Record<string, unknown>) {
  const lines: string[] = [];
  const searchQueries = data.search_query;
  const queries: string[] = [];

  if (Array.isArray(searchQueries)) {
    for (const entry of searchQueries) {
      if (isRecord(entry)) {
        const query = getString(entry.q)?.trim();
        if (query) {
          queries.push(query);
        }
      } else if (typeof entry === "string" && entry.trim()) {
        queries.push(entry.trim());
      }
    }
  }

  if (queries.length > 0) {
    lines.push("Search tool invoked with queries:");
    lines.push(...queries.map((query) => `- ${query}`));
  }

  const additionalItems: string[] = [];

  for (const [key, value] of Object.entries(data)) {
    if (key === "search_query" || key === "response_length") {
      continue;
    }

    if (
      typeof value === "string" ||
      typeof value === "number" ||
      typeof value === "boolean"
    ) {
      const valueText = String(value).trim();
      if (valueText) {
        additionalItems.push(`${key}: ${valueText}`);
      }
    }
  }

  if (additionalItems.length > 0) {
    if (lines.length === 0) {
      lines.push("Tool parameters:");
    }
    lines.push(...additionalItems.map((item) => `- ${item}`));
  }

  return lines.length > 0 ? lines.join("\n") : null;
}

export function buildAssetFilename(
  messageId: string | null,
  index: number,
  mimeType: string | null,
): string {
  const base = (messageId ?? "asset").split("-")[0] || "asset";
  const extension = mimeType?.includes("/")
    ? mimeType.split("/").at(-1)
    : null;

  return `${base}-${index}.${extension || "bin"}`;
}

function makeCodeFence(body: string, language: string): string {
  const longestBacktickRun = Math.max(
    0,
    ...(body.match(/`+/g) ?? []).map((match) => match.length),
  );
  const fence = "`".repeat(Math.max(3, longestBacktickRun + 1));

  return `${fence}${language}\n${body}\n${fence}`;
}

function flattenMessageContent(
  messageId: string | null,
  content: Record<string, unknown>,
  message: Record<string, unknown>,
): [string, ChatGptShareAsset[]] {
  const contentType = getString(content.content_type);
  const assets: ChatGptShareAsset[] = [];

  function renderAssetReference(asset: ChatGptShareAsset): string {
    const relativeDir = asset.assetType === "image" ? "images" : "attachments";
    const relativePath = `${relativeDir}/${asset.filename}`;

    if (asset.downloadable) {
      return asset.assetType === "image"
        ? `![${asset.filename}](${relativePath})`
        : `[${asset.filename}](${relativePath})`;
    }

    const label = asset.description ?? asset.filename;
    const source = asset.url || "unavailable source";
    const kind = asset.assetType === "image" ? "Image" : "Attachment";

    return `*${kind} '${label}' not included in export (source: ${source}).*`;
  }

  function finalize(text: string): [string, ChatGptShareAsset[]] {
    const metadata = isRecord(message.metadata) ? message.metadata : {};
    const attachmentLines: string[] = [];
    const attachments = metadata.attachments;

    if (Array.isArray(attachments)) {
      for (const attachment of attachments) {
        if (!isRecord(attachment)) {
          continue;
        }

        const url = getString(attachment.download_url) ??
          getString(attachment.file_url);
        if (!url) {
          continue;
        }

        const mimeType = getString(attachment.mime_type);
        const name = getString(attachment.name);
        const filename =
          name ?? buildAssetFilename(messageId, assets.length, mimeType);
        const rawAssetType =
          getString(attachment.file_type) ??
          getString(attachment.type) ??
          "file";
        const description =
          getString(attachment.title) ?? getString(attachment.name);

        assets.push({
          assetType: rawAssetType.toLowerCase().includes("image")
            ? "image"
            : "file",
          url,
          filename,
          description,
          downloadable: url.toLowerCase().startsWith("http"),
        });
        attachmentLines.push(renderAssetReference(assets.at(-1)!));
      }
    }

    let combined = text.trim();
    if (attachmentLines.length > 0) {
      combined = `${combined}${combined ? "\n\n" : ""}${attachmentLines.join("\n")}`;
    }

    return [stripCitationTokens(combined), assets];
  }

  if (contentType === "text") {
    const parsedParts: string[] = [];
    const parts = content.parts;

    if (Array.isArray(parts)) {
      for (const part of parts) {
        if (typeof part !== "string") {
          continue;
        }

        const cleaned = stripPrivateUse(part).replace(/^\n+|\n+$/g, "");
        let parsed = cleaned;

        if (cleaned.startsWith("{") && cleaned.endsWith("}")) {
          try {
            const maybeJson = JSON.parse(cleaned) as unknown;
            if (isRecord(maybeJson)) {
              parsed =
                getString(maybeJson.response) ??
                getString(maybeJson.content) ??
                cleaned;
            }
          } catch {}
        }

        parsedParts.push(parsed);
      }
    }

    return finalize(parsedParts.filter(Boolean).join("\n\n"));
  }

  if (contentType === "code") {
    const language = getString(content.language);
    const codeText = getString(content.text) ?? "";
    const lang = language && language !== "unknown" ? language : "";
    let body = codeText.replace(/\n+$/g, "");

    if (body) {
      try {
        const maybeJson = JSON.parse(body) as unknown;
        if (isRecord(maybeJson)) {
          const summary = summarizeToolPayload(maybeJson);
          if (summary) {
            return finalize(summary);
          }

          const cleaned = Object.fromEntries(
            Object.entries(maybeJson).filter(
              ([key]) => key !== "response_length",
            ),
          );

          if (Object.keys(cleaned).length === 0) {
            return finalize("");
          }

          body = JSON.stringify(cleaned, null, 2);
        }
      } catch {}
    }

    return finalize(body ? makeCodeFence(body, lang) : "");
  }

  if (contentType === "thoughts") {
    const thoughts: string[] = [];
    const thoughtItems = content.thoughts;

    if (Array.isArray(thoughtItems)) {
      for (const thought of thoughtItems) {
        if (!isRecord(thought)) {
          continue;
        }

        const summary = getString(thought.summary);
        const detail = getString(thought.content);
        const combined = [summary, detail].filter(Boolean).join(": ");

        if (combined) {
          thoughts.push(`_${combined}_`);
        }
      }
    }

    return finalize(thoughts.join("\n\n"));
  }

  if (contentType === "reasoning_recap") {
    const recap = getString(content.content) ?? "";
    return finalize(recap ? `_${recap.trim()}_` : "");
  }

  if (contentType === "model_editable_context") {
    return finalize(getString(content.model_set_context)?.trim() ?? "");
  }

  if (contentType === "multimodal_text") {
    const segments: string[] = [];
    const parts = content.parts;

    if (Array.isArray(parts)) {
      for (const part of parts) {
        if (typeof part === "string") {
          segments.push(stripPrivateUse(part));
          continue;
        }

        if (!isRecord(part)) {
          continue;
        }

        const partType = getString(part.content_type) ?? getString(part.type);

        if (partType === "text") {
          const text = part.text;

          if (Array.isArray(text)) {
            segments.push(
              ...text
                .filter((item): item is string => typeof item === "string")
                .map(stripPrivateUse),
            );
          } else if (typeof text === "string") {
            segments.push(stripPrivateUse(text));
          }
        } else if (partType === "image_asset_pointer" || partType === "file") {
          const pointer = getString(part.asset_pointer);
          if (!pointer) {
            continue;
          }

          const mimeType = getString(part.mime_type);
          const filename = buildAssetFilename(
            messageId,
            assets.length,
            mimeType,
          );

          assets.push({
            assetType: partType.includes("image") ? "image" : "file",
            url: pointer,
            filename,
            description: null,
            downloadable: pointer.toLowerCase().startsWith("http"),
          });
          segments.push(renderAssetReference(assets.at(-1)!));
        }
      }
    }

    return finalize(
      segments
        .map((segment) => segment.trim())
        .filter(Boolean)
        .join("\n\n"),
    );
  }

  if (contentType === "tool_response") {
    return finalize(stripPrivateUse(getString(content.output) ?? ""));
  }

  if (Array.isArray(content.parts)) {
    return finalize(
      content.parts
        .filter(Boolean)
        .map((part) => stripPrivateUse(String(part)))
        .join("\n\n")
        .trim(),
    );
  }

  return finalize("");
}

function findRecordByShape(
  value: unknown,
  predicate: (record: Record<string, unknown>) => boolean,
  maxDepth = 8,
): Record<string, unknown> | null {
  const seen = new Set<object>();

  function walk(node: unknown, depth: number): Record<string, unknown> | null {
    if (depth > maxDepth || node === null || typeof node !== "object") {
      return null;
    }
    if (seen.has(node as object)) return null;
    seen.add(node as object);

    if (isRecord(node)) {
      if (predicate(node)) return node;
      for (const child of Object.values(node)) {
        const found = walk(child, depth + 1);
        if (found) return found;
      }
    } else if (Array.isArray(node)) {
      for (const child of node) {
        const found = walk(child, depth + 1);
        if (found) return found;
      }
    }
    return null;
  }

  return walk(value, 0);
}

function looksLikeConversationData(record: Record<string, unknown>): boolean {
  return (
    (Array.isArray(record.linear_conversation) &&
      record.linear_conversation.length > 0) ||
    (isRecord(record.mapping) && Object.keys(record.mapping).length > 0)
  );
}

function parseConversationData(
  data: Record<string, unknown>,
  options: {
    shareId: string;
    fallbackAuthorName?: string;
  },
): ChatGptShareConversation {
  const model = isRecord(data.model) ? data.model : {};
  const mapping = isRecord(data.mapping) ? data.mapping : {};
  const sequence = Array.isArray(data.linear_conversation)
    ? data.linear_conversation
    : [];
  const replies: ChatGptShareReply[] = [];

  for (const entry of sequence) {
    if (!isRecord(entry)) {
      continue;
    }

    const nodeId = getString(entry.id);
    const mappedNode = nodeId ? mapping[nodeId] : null;
    const node = isRecord(mappedNode) ? mappedNode : entry;
    const message = isRecord(node.message) ? node.message : null;

    if (!message) {
      continue;
    }

    const author = isRecord(message.author) ? message.author : {};
    const role = getString(author.role);
    if (role === "system") {
      continue;
    }

    const content = isRecord(message.content) ? message.content : null;
    if (!content) {
      continue;
    }

    const [statement, assets] = flattenMessageContent(
      getString(message.id),
      content,
      message,
    );

    if (!statement && assets.length === 0) {
      continue;
    }

    const replyType: ChatGptShareReplyType =
      role === "user" || role === "tool" ? role : "assistant";
    const createdAt = getNumber(message.create_time);
    const authorName =
      role === "user" && options.fallbackAuthorName
        ? options.fallbackAuthorName
        : authorNameForRole(role);

    replies.push({
      authorName,
      type: replyType,
      statement,
      createdAt,
      assets,
    });
  }

  return {
    shareId: options.shareId,
    aiModel: getString(model.slug) ?? "",
    title: getString(data.title) ?? "",
    updatedAt: getNumber(data.update_time),
    replies,
  };
}

export function parseModernShare(html: string): ChatGptShareConversation {
  const loader = extractLoaderPayload(html);
  if (!loader) {
    throw new ChatGptShareParseError("Modern share payload not found.");
  }

  const decoded = decodeLoader(loader);
  const loaderData = isRecord(decoded.loaderData) ? decoded.loaderData : {};
  const route = isRecord(loaderData["routes/share.$shareId.($action)"])
    ? loaderData["routes/share.$shareId.($action)"]
    : {};
  const serverResponse = isRecord(route.serverResponse)
    ? route.serverResponse
    : {};
  const routedData = isRecord(serverResponse.data) ? serverResponse.data : null;
  const data = routedData ?? findRecordByShape(decoded, looksLikeConversationData);

  if (!data) {
    throw new ChatGptShareParseError("Modern share data not found.");
  }

  const shareNode = findRecordByShape(
    decoded,
    (record) => typeof record.sharedConversationId === "string",
  );

  return parseConversationData(data, {
    shareId:
      getString(route.sharedConversationId) ??
      getString(shareNode?.sharedConversationId) ??
      getString(data.conversation_id) ??
      "shared",
  });
}

export function parseLegacyShare(html: string): ChatGptShareConversation {
  const nextData = extractScripts(html).find(
    ({ attrs }) => attrs.id === "__NEXT_DATA__",
  )?.text;

  if (!nextData) {
    throw new ChatGptShareParseError("Legacy share payload not found.");
  }

  let payload: unknown;

  try {
    payload = JSON.parse(nextData) as unknown;
  } catch {
    throw new ChatGptShareParseError("Legacy share payload is invalid.");
  }
  const props = isRecord(payload) && isRecord(payload.props)
    ? payload.props
    : {};
  const pageProps = isRecord(props.pageProps) ? props.pageProps : {};
  const serverResponse = isRecord(pageProps.serverResponse)
    ? pageProps.serverResponse
    : {};
  const routedData = isRecord(serverResponse.data) ? serverResponse.data : null;
  const data = routedData ?? findRecordByShape(payload, looksLikeConversationData);

  if (!data) {
    throw new ChatGptShareParseError("Legacy share data not found.");
  }

  return parseConversationData(data, {
    shareId: getString(data.conversation_id) ?? "shared",
    fallbackAuthorName: getString(data.author_name) ?? "User",
  });
}

export function parseChatGptShareHtml(html: string): ChatGptShareConversation {
  try {
    return parseModernShare(html);
  } catch (error) {
    if (!(error instanceof ChatGptShareParseError)) {
      throw error;
    }
  }

  return parseLegacyShare(html);
}

export async function fetchChatGptShareHtml(url: string | URL) {
  const shareUrl = typeof url === "string" ? new URL(url) : url;
  const response = await fetch(shareUrl, {
    headers: CHATGPT_SHARE_HEADERS,
  });

  if (!response.ok) {
    if (
      response.status === 403 &&
      shareUrl.hostname.endsWith("chatgpt.com") &&
      shareUrl.pathname.startsWith("/c/")
    ) {
      throw new ChatGptShareAccessError(
        "The provided link appears to be a private conversation. Open it while logged in and copy the public https://chatgpt.com/share/... link instead.",
      );
    }

    throw new ChatGptShareFetchError(
      `Failed to fetch the shared ChatGPT conversation (${response.status}).`,
      response.status,
    );
  }

  return response.text();
}

export async function fetchChatGptShare(url: string | URL) {
  const html = await fetchChatGptShareHtml(url);
  return parseChatGptShareHtml(html);
}

function formatUpdatedAt(updatedAt: number): string {
  return new Date(updatedAt * 1000).toISOString().replace("T", " ").slice(0, 19);
}

export function chatGptShareToMarkdown(
  chat: ChatGptShareConversation,
): string {
  const lines = [`# ${chat.title || "ChatGPT conversation"}`];
  const metaBits: string[] = [];

  if (chat.updatedAt) {
    metaBits.push(formatUpdatedAt(chat.updatedAt));
  }

  if (chat.aiModel) {
    metaBits.push(`Model: ${chat.aiModel}`);
  }

  if (metaBits.length > 0) {
    lines.push(`_${metaBits.join(" | ")}_`);
  }

  lines.push("");

  for (const reply of chat.replies) {
    lines.push(`### ${reply.authorName || authorNameForRole(reply.type)}`);
    lines.push(reply.statement.trim());
    lines.push("");
  }

  return `${lines.map((line) => line.trimEnd()).join("\n").trimEnd()}\n`;
}
