"""Guarded final test-fixture fix for PR #46; removed before final review."""
from pathlib import Path

path = Path("crates/aiks-core/tests/provider_runtime_regressions.rs")
text = path.read_text(encoding="utf-8")
old = "    let config = Config::default();\n    let worker = PipelineWorker::start_with_limit("
new = """    // This regression tests discovery isolation, not a deployed model service.
    // Explicitly disable both networks; product defaults are not test fixtures.
    let mut config = Config::default();
    config.ai.enabled = false;
    config.ai.auto_extract = false;
    config.embedding.enabled = false;
    let worker = PipelineWorker::start_with_limit("""
if new not in text:
    if text.count(old) != 1:
        raise RuntimeError("Expected exactly one provider recovery configuration")
    path.write_text(text.replace(old, new, 1), encoding="utf-8")
    print("Updated", path)
