"""Apply exact SHA-guarded Task 9 Rust candidate; CI formats and emits blobs only."""
from pathlib import Path
import base64,gzip,hashlib,json,subprocess

def git_blob_sha(data):
    return hashlib.sha1(b"blob "+str(len(data)).encode()+b"\0"+data).hexdigest()

HEAD="86ebbbd10b230ed83c4dfd2d1a6234a4b71dd4ee"
subprocess.check_call(["git","merge-base","--is-ancestor",HEAD,"HEAD"])
parts=[]
for p in sorted(Path("scripts/task9-candidate").glob("part-*.txt")):
    parts.append(p.read_text())
raw=gzip.decompress(base64.b64decode("".join(parts)))
obj=json.loads(raw)
assert obj["source_head"]==HEAD
for item in obj["manifest"]:
    path=Path(item["path"]); expected=item["old_sha"]
    if expected is None:
        assert not path.exists(),(str(path),"unexpected existing file")
    else:
        actual=git_blob_sha(path.read_bytes())
        assert actual==expected,(str(path),actual,expected)
for path,encoded in obj["payload"].items():
    p=Path(path); p.parent.mkdir(parents=True,exist_ok=True)
    p.write_bytes(gzip.decompress(base64.b64decode(encoded)))
print("Applied SHA-guarded Task 9 Rust candidate; branch is unchanged by this workflow.")
