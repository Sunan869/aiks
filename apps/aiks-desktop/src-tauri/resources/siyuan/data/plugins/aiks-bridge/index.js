const { Plugin, getAllEditor, openTab } = require("siyuan");

const PROTOCOL_VERSION = 1;
const AIKS_EVENT_CHANNEL = "aiks-workbench-event";
const BRIDGE_READY_MAX_ATTEMPTS = 20;
const BRIDGE_READY_RETRY_DELAY_MS = 250;
const ACTIONS = new Set([
  "showKnowledgeRoot",
  "showSessionRoot",
  "openDocument",
  "openBlock",
  "setWorkspaceMode",
  "refreshDocument",
  "aiAssistResult",
]);

const EDIT_KEYS = new Set(["Backspace", "Delete", "Enter", "Tab"]);

function safeId(value) {
  return typeof value === "string" && value.trim() ? value.trim() : null;
}

function escapeSelector(value) {
  if (window.CSS?.escape) {
    return window.CSS.escape(value);
  }
  return value.replace(/["\\]/g, "\\$&");
}

function createdDocumentId(data) {
  const direct = safeId(data?.id) || safeId(data?.rootID);
  if (direct) return direct;
  if (typeof data?.path !== "string") return null;
  const filename = data.path.split("/").filter(Boolean).pop() || "";
  return safeId(filename.endsWith(".sy") ? filename.slice(0, -3) : filename);
}

class SiyuanAdapter {
  constructor(app) {
    this.app = app;
    this.readOnly = false;
    this.disabledEditors = new Set();
    this.readOnlyRefreshTimers = new Set();
  }

  openDocument(docId) {
    const id = safeId(docId);
    if (!id) return false;
    try {
      openTab({ app: this.app, doc: { id } });
      this.scheduleReadOnlyRefresh();
      return true;
    } catch (error) {
      console.warn("[AIKS Bridge] openDocument failed", error);
      return false;
    }
  }

  focusBlock(docId, blockId) {
    const doc = safeId(docId);
    const block = safeId(blockId);
    if (!doc || !block) return false;
    this.openDocument(doc);
    window.setTimeout(() => {
      try {
        const element = document.querySelector(
          `[data-node-id="${escapeSelector(block)}"]`,
        );
        if (element instanceof HTMLElement) {
          element.scrollIntoView({ block: "center", behavior: "smooth" });
          element.focus({ preventScroll: true });
        }
      } catch (error) {
        console.warn("[AIKS Bridge] focusBlock failed", error);
      }
    }, 120);
    return true;
  }

  applyEditorReadOnly() {
    if (!this.readOnly) return;
    let editors = [];
    try {
      editors = getAllEditor();
    } catch (error) {
      console.warn("[AIKS Bridge] getAllEditor failed", error);
      return;
    }
    for (const editor of editors) {
      if (!editor || this.disabledEditors.has(editor)) continue;
      if (typeof editor.disable === "function") {
        try {
          editor.disable();
          this.disabledEditors.add(editor);
        } catch (error) {
          console.warn("[AIKS Bridge] failed to disable Protyle", error);
        }
      }
    }
  }

  restoreEditors() {
    for (const editor of this.disabledEditors) {
      if (typeof editor?.enable !== "function") continue;
      try {
        editor.enable();
      } catch (error) {
        console.warn("[AIKS Bridge] failed to re-enable Protyle", error);
      }
    }
    this.disabledEditors.clear();
  }

  scheduleReadOnlyRefresh() {
    if (!this.readOnly) return;
    this.applyEditorReadOnly();
    for (const delay of [0, 120, 500]) {
      const timer = window.setTimeout(() => {
        this.readOnlyRefreshTimers.delete(timer);
        this.applyEditorReadOnly();
      }, delay);
      this.readOnlyRefreshTimers.add(timer);
    }
  }

  setReadOnly(value) {
    const next = Boolean(value);
    this.readOnly = next;
    document.documentElement.dataset.aiksReadonly = next ? "true" : "false";
    if (next) {
      this.scheduleReadOnlyRefresh();
    } else {
      this.restoreEditors();
    }
    return this.readOnly;
  }

  clearReadOnlyTimers() {
    for (const timer of this.readOnlyRefreshTimers) {
      window.clearTimeout(timer);
    }
    this.readOnlyRefreshTimers.clear();
  }

  refreshDocument(docId) {
    const id = safeId(docId);
    if (!id) return false;
    return this.openDocument(id);
  }

  applyAiksLayout() {
    try {
      document.documentElement.classList.add("aiks-embedded-workbench");
      return true;
    } catch (error) {
      document.documentElement.classList.remove("aiks-embedded-workbench");
      console.warn("[AIKS Bridge] layout adapter disabled", error);
      return false;
    }
  }

  clearAiksLayout() {
    this.clearReadOnlyTimers();
    this.restoreEditors();
    document.documentElement.classList.remove("aiks-embedded-workbench");
    delete document.documentElement.dataset.aiksReadonly;
  }
}

class AIKSBridgePlugin extends Plugin {
  onload() {
    const injectedNonce = window.__AIKS_WORKBENCH_NONCE__;
    this.runtimeNonce = typeof injectedNonce === "string" && injectedNonce
      ? injectedNonce
      : null;
    this.mode = "knowledge";
    this.changeTimers = new Map();
    this.aiAssistRequests = new Map();
    this.bridgeReadyRetryTimer = null;
    this.adapter = new SiyuanAdapter(this.app);
    this.adapter.applyAiksLayout();
    this.askAiksButton = null;
    this.askAiksStyle = null;
    this.mountAskAiksLauncher();

    this.onMessage = (event) => this.handleMessage(event);
    this.onBeforeInput = (event) => this.blockEdit(event);
    this.onPaste = (event) => this.blockEdit(event);
    this.onDrop = (event) => this.blockEdit(event);
    this.onKeyDown = (event) => this.blockKeyDown(event);
    this.onEditorInput = (event) => this.handleEditorInput(event);
    this.onKernelMessage = (event) => this.handleKernelMessage(event);

    window.addEventListener("message", this.onMessage);
    document.addEventListener("beforeinput", this.onBeforeInput, true);
    document.addEventListener("paste", this.onPaste, true);
    document.addEventListener("drop", this.onDrop, true);
    document.addEventListener("keydown", this.onKeyDown, true);
    document.addEventListener("input", this.onEditorInput, true);
    this.eventBus.on("ws-main", this.onKernelMessage);

    this.readOnlyObserver = new MutationObserver(() => {
      if (this.adapter?.readOnly) {
        this.adapter.scheduleReadOnlyRefresh();
      }
    });
    this.readOnlyObserver.observe(document.body, { childList: true, subtree: true });

    const bridge = {
      protocolVersion: 1,
      adapter: this.adapter,
      get mode() {
        return document.documentElement.dataset.aiksWorkspaceMode || "knowledge";
      },
      requestAiAssist: (docId, operation) => this.requestAiAssist(docId, operation),
    };
    window.__AIKS_BRIDGE__ = bridge;
    this.setMode("knowledge");
    this.emit("bridgeReady", { mode: this.mode });
  }

  onunload() {
    window.removeEventListener("message", this.onMessage);
    document.removeEventListener("beforeinput", this.onBeforeInput, true);
    document.removeEventListener("paste", this.onPaste, true);
    document.removeEventListener("drop", this.onDrop, true);
    document.removeEventListener("keydown", this.onKeyDown, true);
    document.removeEventListener("input", this.onEditorInput, true);
    this.eventBus.off("ws-main", this.onKernelMessage);
    this.readOnlyObserver?.disconnect?.();
    if (this.bridgeReadyRetryTimer !== null) {
      window.clearTimeout(this.bridgeReadyRetryTimer);
      this.bridgeReadyRetryTimer = null;
    }
    for (const timer of this.changeTimers?.values?.() || []) {
      window.clearTimeout(timer);
    }
    this.changeTimers?.clear?.();
    for (const pending of this.aiAssistRequests?.values?.() || []) {
      window.clearTimeout(pending.timer);
      pending.reject(new Error("AIKS AI Assist bridge unloaded"));
    }
    this.aiAssistRequests?.clear?.();
    this.unmountAskAiksLauncher();
    this.adapter?.clearAiksLayout();
    delete window.__AIKS_BRIDGE__;
  }

  mountAskAiksLauncher() {
    if (this.askAiksButton?.isConnected) return;

    const style = document.createElement("style");
    style.id = "aiks-ask-launcher-style";
    style.textContent = `
      #aiks-ask-launcher {
        position: fixed;
        right: 20px;
        bottom: 16px;
        z-index: 2147483647;
        display: inline-flex;
        align-items: center;
        gap: 10px;
        border: 1px solid rgba(59, 130, 246, 0.16);
        border-radius: 9999px;
        padding: 10px 16px;
        background: var(--b3-theme-background, #fff);
        color: #2563eb;
        box-shadow: 0 10px 30px rgba(37, 99, 235, 0.20);
        font-family: inherit;
        font-size: 14px;
        font-weight: 600;
        line-height: 20px;
        cursor: pointer;
        user-select: none;
        transition: transform 150ms ease, background 150ms ease, border-color 150ms ease, box-shadow 150ms ease;
      }
      #aiks-ask-launcher:hover {
        transform: translateY(-2px);
        border-color: rgba(59, 130, 246, 0.28);
        background: color-mix(in srgb, var(--b3-theme-primary-lightest, #eff6ff) 72%, var(--b3-theme-background, #fff));
        box-shadow: 0 14px 34px rgba(37, 99, 235, 0.26);
      }
      #aiks-ask-launcher:active {
        transform: translateY(0);
      }
      #aiks-ask-launcher:focus-visible {
        outline: 2px solid rgba(37, 99, 235, 0.45);
        outline-offset: 2px;
      }
      #aiks-ask-launcher .aiks-ask-icon {
        width: 28px;
        height: 28px;
        flex: 0 0 28px;
        display: inline-flex;
        align-items: center;
        justify-content: center;
        border-radius: 9999px;
        background: #2563eb;
        color: #fff;
        box-shadow: 0 1px 2px rgba(15, 23, 42, 0.10);
      }
      #aiks-ask-launcher .aiks-ask-icon svg {
        width: 14px;
        height: 14px;
        display: block;
      }
    `;
    document.head.appendChild(style);

    const button = document.createElement("button");
    button.id = "aiks-ask-launcher";
    button.type = "button";
    button.title = "基于 AIKS 知识库和 AI 对话记录提问";
    button.setAttribute("aria-label", "问 AIKS");
    button.innerHTML = `
      <span class="aiks-ask-icon" aria-hidden="true">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M12 3l1.3 3.7L17 8l-3.7 1.3L12 13l-1.3-3.7L7 8l3.7-1.3L12 3z"></path>
          <path d="M19 14l.8 2.2L22 17l-2.2.8L19 20l-.8-2.2L16 17l2.2-.8L19 14z"></path>
          <path d="M5 13l.9 2.6L8.5 16.5l-2.6.9L5 20l-.9-2.6-2.6-.9 2.6-.9L5 13z"></path>
        </svg>
      </span>
      <span>问 AIKS</span>
    `;
    button.addEventListener("click", (event) => {
      event.preventDefault();
      event.stopPropagation();
      this.emit("requestAskAiks");
    });

    document.body.appendChild(button);
    this.askAiksStyle = style;
    this.askAiksButton = button;
  }

  unmountAskAiksLauncher() {
    this.askAiksButton?.remove?.();
    this.askAiksStyle?.remove?.();
    this.askAiksButton = null;
    this.askAiksStyle = null;
  }

  handleMessage(event) {
    if (event.source !== window || event.origin !== window.location.origin) return;
    const message = event.data;
    if (!message || typeof message !== "object" || typeof message.action !== "string") return;
    if (message.version !== PROTOCOL_VERSION || !ACTIONS.has(message.action)) return;
    if (typeof message.nonce !== "string" || !message.nonce) return;

    if (this.runtimeNonce === null) {
      this.runtimeNonce = message.nonce;
      this.emit("bridgeReady", { mode: this.mode });
    } else if (message.nonce !== this.runtimeNonce) {
      return;
    }

    const payload = message.payload && typeof message.payload === "object"
      ? message.payload
      : {};
    this.dispatch(message.action, payload);
  }

  handleKernelMessage(event) {
    const message = event?.detail;
    if (!message || typeof message !== "object") return;

    switch (message.cmd) {
      case "create": {
        const docId = createdDocumentId(message.data);
        if (docId) {
          this.emit("documentCreated", { docId });
        }
        break;
      }
      case "removeDoc": {
        const ids = Array.isArray(message.data?.ids) ? message.data.ids : [];
        for (const rawId of ids) {
          const docId = safeId(rawId);
          if (docId) {
            this.emit("documentDeleted", { docId });
          }
        }
        break;
      }
      default:
        break;
    }
  }

  dispatch(action, payload) {
    switch (action) {
      case "showKnowledgeRoot":
        this.setMode("knowledge");
        break;
      case "showSessionRoot":
        this.setMode("session");
        break;
      case "openDocument":
        if (this.adapter.openDocument(payload.docId)) {
          this.emit("documentOpened", { docId: safeId(payload.docId) });
        }
        break;
      case "openBlock":
        if (this.adapter.focusBlock(payload.docId, payload.blockId)) {
          this.emit("documentOpened", {
            docId: safeId(payload.docId),
            blockId: safeId(payload.blockId),
          });
        }
        break;
      case "setWorkspaceMode":
        this.setMode(payload.mode);
        break;
      case "aiAssistResult":
        this.resolveAiAssist(payload);
        break;
      case "refreshDocument":
        this.adapter.refreshDocument(payload.docId);
        break;
      default:
        break;
    }
  }

  requestAiAssist(docId, operation) {
    const id = safeId(docId);
    const op = safeId(operation);
    if (!id || !op) {
      return Promise.reject(new Error("AI Assist requires docId and operation"));
    }
    const requestId = window.crypto?.randomUUID?.() ||
      `aiks-ai-${Date.now()}-${Math.random().toString(16).slice(2)}`;
    return new Promise((resolve, reject) => {
      const timer = window.setTimeout(() => {
        this.aiAssistRequests.delete(requestId);
        reject(new Error("AI Assist request timed out"));
      }, 60_000);
      this.aiAssistRequests.set(requestId, {resolve, reject, timer});
      this.emit("requestAiAssist", {requestId, docId: id, operation: op});
    });
  }

  resolveAiAssist(payload) {
    const requestId = safeId(payload?.requestId);
    if (!requestId) return false;
    const pending = this.aiAssistRequests.get(requestId);
    if (!pending) return false;
    this.aiAssistRequests.delete(requestId);
    window.clearTimeout(pending.timer);
    if (payload?.ok === true) {
      pending.resolve(payload.suggestion || {});
    } else {
      pending.reject(new Error(safeId(payload?.error) || "AI Assist failed"));
    }
    return true;
  }

  setMode(mode) {
    if (mode !== "knowledge" && mode !== "session") return false;
    this.mode = mode;
    document.documentElement.dataset.aiksWorkspaceMode = mode;
    this.adapter.setReadOnly(mode === "session");
    this.emit("workspaceModeChanged", { mode });
    return true;
  }

  isEditorTarget(target) {
    return target instanceof Element && Boolean(
      target.closest('.protyle-wysiwyg, [contenteditable="true"]'),
    );
  }

  blockEdit(event) {
    if (!this.adapter?.readOnly || !this.isEditorTarget(event.target)) return;
    event.preventDefault();
    event.stopImmediatePropagation();
  }

  blockKeyDown(event) {
    if (!this.adapter?.readOnly || !this.isEditorTarget(event.target)) return;
    const lower = String(event.key || "").toLowerCase();
    const editingShortcut = (event.ctrlKey || event.metaKey) &&
      ["v", "x", "z", "y"].includes(lower);
    const printable = !event.ctrlKey && !event.metaKey && !event.altKey &&
      String(event.key || "").length === 1;
    if (!EDIT_KEYS.has(event.key) && !editingShortcut && !printable) return;
    event.preventDefault();
    event.stopImmediatePropagation();
  }

  handleEditorInput(event) {
    if (this.mode !== "knowledge" || !this.isEditorTarget(event.target)) return;
    const block = event.target instanceof Element
      ? event.target.closest("[data-node-id]")
      : null;
    const root = event.target instanceof Element
      ? event.target.closest(".protyle")?.querySelector("[data-root-id]")
      : null;
    const docId = root?.getAttribute("data-root-id") ||
      block?.getAttribute("data-node-id") || null;
    if (docId) {
      this.scheduleDocumentChanged(docId);
    }
  }

  scheduleDocumentChanged(docId) {
    const id = safeId(docId);
    if (!id) return;
    const existing = this.changeTimers.get(id);
    if (existing) window.clearTimeout(existing);
    const timer = window.setTimeout(() => {
      this.changeTimers.delete(id);
      this.emit("documentChanged", { docId: id });
    }, 500);
    this.changeTimers.set(id, timer);
  }

  refreshRuntimeNonce() {
    if (this.runtimeNonce !== null) return this.runtimeNonce;
    const injectedNonce = safeId(window.__AIKS_WORKBENCH_NONCE__);
    if (injectedNonce) {
      this.runtimeNonce = injectedNonce;
    }
    return this.runtimeNonce;
  }

  scheduleBridgeReadyRetry(envelope, attempt) {
    if (this.bridgeReadyRetryTimer !== null) {
      window.clearTimeout(this.bridgeReadyRetryTimer);
    }
    this.bridgeReadyRetryTimer = window.setTimeout(() => {
      this.bridgeReadyRetryTimer = null;
      this.retryBackendEmit("bridgeReady", envelope, attempt);
    }, BRIDGE_READY_RETRY_DELAY_MS);
  }

  retryBackendEmit(eventName, envelope, attempt = 1) {
    if (eventName === "bridgeReady") {
      this.refreshRuntimeNonce();
      envelope.nonce = this.runtimeNonce;
    }

    const invoke = window.__TAURI_INTERNALS__?.invoke;
    if (this.runtimeNonce && typeof invoke === "function") {
      Promise.resolve(invoke("plugin:event|emit", {
        event: AIKS_EVENT_CHANNEL,
        payload: envelope,
      })).catch((error) => {
        if (eventName === "bridgeReady" && attempt < BRIDGE_READY_MAX_ATTEMPTS) {
          this.scheduleBridgeReadyRetry(envelope, attempt + 1);
          return;
        }
        console.debug("[AIKS Bridge] backend event channel unavailable", error);
      });
      return;
    }

    if (eventName === "bridgeReady" && attempt < BRIDGE_READY_MAX_ATTEMPTS) {
      this.scheduleBridgeReadyRetry(envelope, attempt + 1);
    } else if (eventName === "bridgeReady") {
      console.warn("[AIKS Bridge] bridgeReady backend handshake timed out", {
        nonceReady: Boolean(this.runtimeNonce),
        ipcReady: typeof invoke === "function",
      });
    }
  }

  emit(eventName, payload = {}) {
    const detail = {};
    for (const [key, value] of Object.entries(payload)) {
      if (typeof value === "string" || typeof value === "number" ||
          typeof value === "boolean" || value === null) {
        detail[key] = value;
      }
    }
    const envelope = {
      source: "aiks-bridge",
      version: PROTOCOL_VERSION,
      nonce: this.runtimeNonce,
      event: eventName,
      payload: detail,
    };
    window.postMessage(envelope, window.location.origin);
    this.retryBackendEmit(eventName, envelope);
  }
}

module.exports = AIKSBridgePlugin;
