import type { SourceDescriptor } from "./provider-catalog-model";

// Synthetic development fixture, checked against the actual Core catalog in tests.
// Production source definitions always come from get_source_descriptors.
export function mockSourceDescriptors(): SourceDescriptor[] {
  const pairs = [
    ["claude_code", "Claude Code", "claude"], ["codex", "Codex", "codex"],
    ["gemini_cli", "Gemini CLI", "gemini"], ["opencode", "OpenCode", "opencode"],
    ["workbuddy", "WorkBuddy", "workbuddy"], ["antigravity", "Antigravity", "antigravity"],
    ["cursor", "Cursor", "cursor"], ["cursor_agent", "Cursor Agent", "cursor_agent"],
    ["cline", "Cline", "cline"], ["roo_code", "Roo Code", "roo_code"],
    ["kilo_code", "Kilo Code", "kilo_code"], ["github_copilot", "GitHub Copilot", "github_copilot"],
    ["kimi_code", "Kimi Code", "kimi_code"], ["qwen_code", "Qwen Code", "qwen_code"],
    ["continue", "Continue", "continue"], ["aider", "Aider", "aider"],
  ];
  return pairs.map(([key, display_name, config_key]) => ({ key, display_name, config_key, enabled: true, paths: [], status: key === "aider" ? "not_configured" : key === "antigravity" ? "unsupported" : "ok", message: "Synthetic development fixture", restart_required: false }));
}
