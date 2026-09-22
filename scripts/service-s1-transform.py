"""One exact correction; existing runner emits a reviewable blob, never a ref."""
from pathlib import Path
import subprocess
p = Path('crates/aiks-core/src/pipeline/worker.rs')
assert subprocess.check_output(['git', 'hash-object', str(p)], text=True).strip() == 'e7e940ea6b4a244c2da0053dfd6a669634c80d07'
s = p.read_text()
old = '''                    // The durable queue is idle: end this discovery cycle so a
                    // later sync/backfill starts from fresh provider metadata.
                    discovery_cache.clear().await;'''
new = '''                    // No claimable row does not mean idle while tasks are
                    // loading or awaiting a model. Keep their discovery cycle.
                    // Once both are drained, the next cycle must refresh.
                    if tasks.is_empty() {
                        discovery_cache.clear().await;
                    }'''
assert s.count(old) == 1
p.write_text(s.replace(old, new))
