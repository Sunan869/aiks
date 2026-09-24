"""Apply exact SHA-guarded Task 8 Rust candidate; CI formats and emits blobs only."""
from pathlib import Path
import base64,gzip,hashlib,json,subprocess

def git_blob_sha(data):
    return hashlib.sha1(b"blob "+str(len(data)).encode()+b"\0"+data).hexdigest()

parts=[]
for p in sorted(Path("scripts/task8-candidate").glob("part-*.txt")):
    parts.append(p.read_text())
raw=gzip.decompress(base64.b64decode("".join(parts)))
obj=json.loads(raw)
new_paths=[]
for item in obj["manifest"]:
    path=Path(item["path"])
    expected=item["old_sha"]
    if expected is None:
        assert not path.exists(), (str(path), "unexpected existing file")
        new_paths.append(str(path))
    else:
        actual=git_blob_sha(path.read_bytes())
        assert actual==expected, (str(path),actual,expected)
for path,encoded in obj["payload"].items():
    p=Path(path);p.parent.mkdir(parents=True,exist_ok=True)
    p.write_bytes(gzip.decompress(base64.b64decode(encoded)))
if new_paths:
    subprocess.check_call(["git","add","-N","--",*new_paths])
print("Applied SHA-guarded Task 8 Rust candidate; branch is unchanged by this workflow.")
