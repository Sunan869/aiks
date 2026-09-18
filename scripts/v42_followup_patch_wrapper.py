from pathlib import Path
import runpy

root = Path(__file__).resolve().parents[1]
patch = root / "scripts" / "v42_followup_patch.py"
text = patch.read_text(encoding="utf-8")
old = '''replace_once(
    "crates/aiks-core/src/sync/engine.rs",
    "                    self.record_extraction_candidate(db, summary, opts, &mut stats);",
    "                    if let Some(candidate) =\\n                        self.record_extraction_candidate(db, summary, opts, &mut stats)\\n                    {\\n                        on_candidate(&candidate);\\n                    }",
)
# second occurrence (Updated)
replace_once(
    "crates/aiks-core/src/sync/engine.rs",
    "                    self.record_extraction_candidate(db, summary, opts, &mut stats);",
    "                    if let Some(candidate) =\\n                        self.record_extraction_candidate(db, summary, opts, &mut stats)\\n                    {\\n                        on_candidate(&candidate);\\n                    }",
)
'''
new = '''candidate_path = "crates/aiks-core/src/sync/engine.rs"
candidate_old = "                    self.record_extraction_candidate(db, summary, opts, &mut stats);"
candidate_new = "                    if let Some(candidate) =\\n                        self.record_extraction_candidate(db, summary, opts, &mut stats)\\n                    {\\n                        on_candidate(&candidate);\\n                    }"
candidate_text = read(candidate_path)
if candidate_text.count(candidate_old) != 2:
    raise RuntimeError(
        f"{candidate_path}: expected exactly two candidate anchors, "
        f"found {candidate_text.count(candidate_old)}"
    )
write(candidate_path, candidate_text.replace(candidate_old, candidate_new))
'''
if text.count(old) != 1:
    raise RuntimeError("original candidate patch block was not found exactly once")
text = text.replace(old, new, 1)
text = text.replace(
    'if count != 4:\n    raise RuntimeError(f"lifecycle AppState anchor count changed: {count}")',
    'if count != 3:\n    raise RuntimeError(f"lifecycle AppState anchor count changed: {count}")',
    1,
)
patch.write_text(text, encoding="utf-8")
runpy.run_path(str(patch), run_name="__main__")
