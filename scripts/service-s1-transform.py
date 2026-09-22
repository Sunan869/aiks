"""Apply the byte-budget correction against an exact source blob."""
from pathlib import Path
import subprocess
p=Path('crates/aiks-core/src/service/query.rs')
assert subprocess.check_output(['git','hash-object',str(p)],text=True).strip()=='e481a5233a703899f43225e4fea8e14a422a003b'
s=p.read_text()
old='length(ki.content)<=1048576'
assert s.count(old)==1
p.write_text(s.replace(old,'length(CAST(ki.content AS BLOB))<=1048576'))
