from pathlib import Path
import re


def replace_once(path: str, old: str, new: str, label: str) -> None:
    target = Path(path)
    text = target.read_text()
    if old not in text:
        raise SystemExit(f"missing replacement target: {label}")
    target.write_text(text.replace(old, new, 1))


def regex_once(path: str, pattern: str, replacement: str, label: str) -> None:
    target = Path(path)
    text = target.read_text()
    next_text, count = re.subn(pattern, replacement, text, count=1, flags=re.S)
    if count != 1:
        raise SystemExit(f"regex replacement count {count}: {label}")
    target.write_text(next_text)


commands = "apps/aiks-desktop/src-tauri/src/workbench/commands.rs"
regex_once(
    commands,
    r'''            WorkbenchAction::ShowBacklinks \{ block_id \} => \{.*?            WorkbenchAction::ShowSearch => \("showSearch", json!\(\{\}\)\),\n''',
    '''            WorkbenchAction::AiAssistResult {
                request_id,
                ok,
                suggestion,
                error,
            } => {
                let request_id = validate_identifier("request_id", &request_id)?;
                (
                    "aiAssistResult",
                    json!({
                        "requestId": request_id,
                        "ok": ok,
                        "suggestion": suggestion,
                        "error": error,
                    }),
                )
            }
''',
    "commands action mapping",
)
regex_once(
    commands,
    r'''#\[tauri::command\]\npub async fn show_workbench_backlinks\(.*?(?=#\[tauri::command\]\npub async fn refresh_siyuan_document)''',
    '',
    "remove native presentation commands",
)

lib = "apps/aiks-desktop/src-tauri/src/lib.rs"
for line in [
    "            workbench::commands::show_workbench_backlinks,\n",
    "            workbench::commands::show_workbench_outline,\n",
    "            workbench::commands::show_workbench_database,\n",
    "            workbench::commands::show_workbench_graph,\n",
    "            workbench::commands::show_workbench_search,\n",
]:
    replace_once(lib, line, "", f"remove {line.strip()}")

events = "apps/aiks-desktop/src-tauri/src/workbench/events.rs"
replace_once(
    events,
    '''    "requestOpenKnowledge",
    "requestShowPipeline",
    "workspaceModeChanged",''',
    '''    "requestOpenKnowledge",
    "requestShowPipeline",
    "requestAiAssist",
    "workspaceModeChanged",''',
    "allow requestAiAssist",
)

mock = "apps/aiks-desktop/src/api/mock.ts"
regex_once(
    mock,
    r'''\n  async showWorkbenchSurface\(surface: WorkspaceMode \| "database" \| "graph"\): Promise<void> \{.*?\n  async hideWorkbench\(\): Promise<void> \{\}''',
    '''
  async hideWorkbench(): Promise<void> {}''',
    "remove mock surface forwarding",
)
regex_once(
    mock,
    r'''\n  async showWorkbenchSearch\(\): Promise<void> \{\}\n\n  async showWorkbenchDatabase\(\): Promise<void> \{\}\n\n  async showWorkbenchGraph\(\): Promise<void> \{\}\n''',
    '\n',
    "remove mock native forwarding",
)

plugin = "apps/aiks-desktop/src-tauri/resources/siyuan/data/plugins/aiks-bridge/index.js"
replace_once(
    plugin,
    '''  "setWorkspaceMode",
  "showBacklinks",
  "showOutline",
  "showDatabase",
  "showGraph",
  "showSearch",
  "refreshDocument",''',
    '''  "setWorkspaceMode",
  "refreshDocument",
  "aiAssistResult",''',
    "plugin action allowlist",
)
regex_once(
    plugin,
    r'''\n  clickFirst\(selectors\) \{.*?(?=\n  refreshDocument\(docId\) \{)''',
    '',
    "remove plugin presentation adapters",
)
replace_once(
    plugin,
    '''    this.mode = "knowledge";
    this.changeTimers = new Map();
    this.adapter = new SiyuanAdapter(this.app);''',
    '''    this.mode = "knowledge";
    this.changeTimers = new Map();
    this.aiAssistRequests = new Map();
    this.adapter = new SiyuanAdapter(this.app);''',
    "plugin request state",
)
replace_once(
    plugin,
    '''      get mode() {
        return document.documentElement.dataset.aiksWorkspaceMode || "knowledge";
      },
    };''',
    '''      get mode() {
        return document.documentElement.dataset.aiksWorkspaceMode || "knowledge";
      },
      requestAiAssist: (docId, operation) => this.requestAiAssist(docId, operation),
    };''',
    "plugin bridge request API",
)
replace_once(
    plugin,
    '''    this.changeTimers?.clear?.();
    this.adapter?.clearAiksLayout();''',
    '''    this.changeTimers?.clear?.();
    for (const pending of this.aiAssistRequests?.values?.() || []) {
      window.clearTimeout(pending.timer);
      pending.reject(new Error("AIKS AI Assist bridge unloaded"));
    }
    this.aiAssistRequests?.clear?.();
    this.adapter?.clearAiksLayout();''',
    "plugin request cleanup",
)
regex_once(
    plugin,
    r'''      case "showBacklinks":.*?      case "showSearch":\n        this\.adapter\.showSearch\(\);\n        break;\n''',
    '''      case "aiAssistResult":
        this.resolveAiAssist(payload);
        break;
''',
    "plugin dispatch cleanup",
)
replace_once(
    plugin,
    '''  setMode(mode) {
''',
    '''  requestAiAssist(docId, operation) {
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
''',
    "plugin request/result methods",
)
