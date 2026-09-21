"""One-shot, guarded edits for PR #46; remove with its write-enabled dev workflow before review."""
from pathlib import Path


def edit(path, fn):
    file = Path(path)
    old = file.read_text(encoding="utf-8")
    new = fn(old)
    if new != old:
        file.write_text(new, encoding="utf-8")
        print("Updated", path)


def once(text, old, new):
    if new in text:
        return text
    if text.count(old) != 1:
        raise RuntimeError("Expected exactly one anchor: " + old[:100])
    return text.replace(old, new, 1)


def fixture(text):
    # Real SiYuan responses have a required msg field. The previous test server
    # omitted it on every endpoint, so all eleven integrations failed before
    # creating an archive; do not weaken the production response decoder.
    text = once(text, "            let response = if path.ends_with(\"/chat/completions\") {", "            let mut response = if path.ends_with(\"/chat/completions\") {")
    text = once(text, "            let body = response.to_string();", """            if path.starts_with("/api/") {
                response["msg"] = json!("");
            }
            let body = response.to_string();""")
    text = once(text, """            } else if path.ends_with("/getBlockAttrs") {
                json!({"code":0,"data":{}})
            } else {
                json!({"code":0,"data":null})
            };""", """            } else if path.ends_with("/getBlockAttrs") {
                json!({"code":0,"data":{}})
            } else if path.ends_with("/query/sql") {
                let stmt = payload["stmt"].as_str().unwrap();
                // Mirror the sink's existence query; an absent document must
                // stay absent, rather than returning a blanket successful row.
                let id = stmt.split("id = '").nth(1).and_then(|s| s.split('\\'').next());
                let found = id.is_some_and(|id| documents.lock().unwrap().contains_key(id));
                json!({"code":0,"data":if found { vec![json!({"box":"kn"})] } else { Vec::<Value>::new() }})
            } else if path.ends_with("/setBlockAttrs") {
                json!({"code":0,"data":null})
            } else {
                panic!("unexpected mock endpoint: {path}");
            };""")
    return text


edit("crates/aiks-core/tests/support/multi_provider_flow.rs", fixture)

# commands.rs::sync_and_extract already returns these seven fields. Share one
# typed response across interface, Tauri transport and browser mock so the new
# source card reports real failures, not a cast or an invented zero fallback.
response_type = """export interface SyncAndExtractResult {
  discovered: number;
  new_count: number;
  updated_count: number;
  unchanged_count: number;
  skipped_count: number;
  failed_count: number;
  extraction_queued: number;
}

"""
edit("apps/aiks-desktop/src/api/types.ts", lambda s: s if "export interface SyncAndExtractResult" in s else response_type + s)
for name in ("index", "tauri", "mock"):
    def api(text):
        old = "Promise<{ discovered: number; new_count: number; updated_count: number }>"
        text = once(text, old, "Promise<SyncAndExtractResult>")
        import_line = 'import type { SyncAndExtractResult } from "./types";\n'
        if import_line not in text:
            text = import_line + text
        return text
    edit(f"apps/aiks-desktop/src/api/{name}.ts", api)
edit("apps/aiks-desktop/src/api/mock.ts", lambda s: once(s,
    "return { discovered: 60, new_count: 0, updated_count: 0 };",
    "return { discovered: 60, new_count: 0, updated_count: 0, unchanged_count: 60, skipped_count: 0, failed_count: 0, extraction_queued: 0 };"))
