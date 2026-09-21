"""Temporary feature-branch test wiring; remove before final review."""
from pathlib import Path

def edit(path,fn):
    p=Path(path);old=p.read_text(encoding='utf-8');new=fn(old)
    if old!=new:p.write_text(new,encoding='utf-8');print('Updated',path)
def wire(s):
    if 'mod multi_provider_flow;' not in s:
        s='#[path = "support/multi_provider_flow.rs"]\nmod multi_provider_flow;\n'+s
        old='    session\n}'
        assert s.count(old)==1
        s=s.replace(old,'    multi_provider_flow::verify(&config, &session).await;\n    session\n}',1)
    return s
edit('crates/aiks-core/tests/multi_provider_acceptance.rs',wire)
