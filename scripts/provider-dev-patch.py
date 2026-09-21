"""Temporary feature-branch mechanical edits; remove before merge review."""
from pathlib import Path
import re

def edit(path, fn):
    file = Path(path)
    old = file.read_text(encoding='utf-8')
    new = fn(old)
    if new != old:
        file.write_text(new, encoding='utf-8')
        print('Updated', path)

edit('crates/aiks-core/src/providers/antigravity.rs', lambda s: s.replace('use serde_json::Value;\n',''))
def dedup(s):
    s = re.sub(r'if path\.extension\(\)\.and_then\(\|x\| x\.to_str\(\)\) == Some\("jsonl"\) \{\s*add_file\(io, &mut files, path\)\?;\s*\} else if path\.extension\(\)\.and_then\(\|x\| x\.to_str\(\)\) == Some\("json"\)\s*&& !io\.exists\(&path\.with_extension\("jsonl"\)\)\?\s*\{', 'if path.extension().and_then(|x| x.to_str()) == Some("jsonl") || (path.extension().and_then(|x| x.to_str()) == Some("json") && !io.exists(&path.with_extension("jsonl"))?) {', s)
    return s
edit('crates/aiks-core/src/providers/local_paths.rs',dedup)
