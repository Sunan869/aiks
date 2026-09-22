"""Deduplicate the shared test module without suppressing Clippy."""
from pathlib import Path
import subprocess
for path, expected, old, new in [
    ('apps/aiks-service/tests/support/mod.rs','8db7ba687ae52d6dcd647ca83ade411dab0d868d','mod fixture;','pub(super) mod fixture;'),
    ('apps/aiks-service/tests/http_contract.rs','aa96b9f1c6aa0dfe549bad8243d89f193169d6be','#[path = "../../../crates/aiks-core/tests/support/service_fixture.rs"]\nmod fixture;','use support::fixture;'),
]:
    p=Path(path)
    assert subprocess.check_output(['git','hash-object',str(p)],text=True).strip()==expected,path
    s=p.read_text()
    assert s.count(old)==1,path
    p.write_text(s.replace(old,new))
