import { Plugin, getAllEditor, openTab } from "siyuan";

const PROTOCOL_VERSION = 1;
const AIKS_EVENT_CHANNEL = "aiks-workbench-event";
const ACTIONS = new Set([
  "showKnowledgeRoot",
  "showSessionRoot",
  "openDocument",
  "openBlock",
  "setWorkspaceMode",
  "showBacklinks",
  "showOutline",
  "showDatabase",
  "showGraph",
  "showSearch",
  "refreshDocument",
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

  clickFirst(selectors) {
    for (const selector of selectors) {
      try {
        const element = document.querySelector(selector);
        if (element instanceof HTMLElement) {
          element.click();
          return true;
        }
      } catch (error) {
        console.debug("[AIKS Bridge] selector unavailable", selector, error);
      }
    }
    return false;
  }

  showBacklinks() {
    return this.clickFirst([
      '[data-type="backlink"]',
      '[data-type="backlinks"]',
      '[data-key="dialog-backlink"]',
    ]);
  }

  showOutline() {
    return this.clickFirst([
      '[data-type="outline"]',
      '[data-key="dialog-outline"]',
      '[aria-label*="Outline"]',
      '[aria-label*="大纲"]',
    ]);
  }

  showDatabase() {
    return this.clickFirst([
      '[data-type="database"]',
      '[data-type="av"]',
      '[aria-label*="Database"]',
      '[aria-label*="数据库"]',
    ]);
  }

  async showGraph() {
    try {
      const host = window.aiksWorkbench;
      if (host && typeof host.openGraph === "function") {
        const opened = await host.openGraph();
        if (opened === true) return true;
      }
    } catch (error) {
      console.warn("[AIKS Bridge] native Graph host API failed", error);
    }

    return this.clickFirst([
      "#barGraph",
      '[data-type="graph"]',
      '[aria-label*="Graph"]',
      '[aria-label*="关系图"]',
    ]);
  }

  showSearch() {
    return this.clickFirst([
      "#barSearch",
      '[data-type="search"]',
      '[aria-label*="Search"]',
      '[aria-label*="搜索"]',
    ]);
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

export default class AIKSBridgePlugin extends Plugin {
  onload() {
    const injectedNonce = window.__AIKS_WORKBENCH_NONCE__;
    this.runtimeNonce = typeof injectedNonce === "string" && injectedNonce
      ? injectedNonce
      : null;
    this.mode = "knowledge";
    this.changeTimers = new Map();
    this.adapter = new SiyuanAdapter(this.app);
    this.adapter.applyAiksLayout();

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
    for (const timer of this.changeTimers?.values?.() || []) {
      window.clearTimeout(timer);
    }
    this.changeTimers?.clear?.();
    this.adapter?.clearAiksLayout();
    delete window.__AIKS_BRIDGE__;
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
      case "showBacklinks":
        this.adapter.showBacklinks(payload.blockId);
        break;
      case "showOutline":
        this.adapter.showOutline();
        break;
      case "showDatabase":
        this.adapter.showDatabase();
        break;
      case "showGraph":
        this.adapter.showGraph();
        break;
      case "showSearch":
        this.adapter.showSearch();
        break;
      case "refreshDocument":
        this.adapter.refreshDocument(payload.docId);
        break;
      default:
        break;
    }
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

    const invoke = window.__TAURI_INTERNALS__?.invoke;
    if (this.runtimeNonce && typeof invoke === "function") {
      Promise.resolve(invoke("plugin:event|emit", {
        event: AIKS_EVENT_CHANNEL,
        payload: envelope,
      })).catch((error) => {
        console.debug("[AIKS Bridge] backend event channel unavailable", error);
      });
    }
  }
}
