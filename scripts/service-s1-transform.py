"""Expose the actual Desktop module to its integration target, no code copies."""
from pathlib import Path
p=Path('apps/aiks-service/tests/desktop_client.rs')
s=p.read_text()
assert s.count('mod service_client;') == 1
p.write_text(s.replace('mod service_client;','pub mod service_client;',1))
