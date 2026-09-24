"""Apply exact SHA-guarded Task 8 compile fixes; CI formats and emits blobs only."""
from pathlib import Path
import hashlib

def blob_sha(data):
    return hashlib.sha1(b"blob "+str(len(data)).encode()+b"\0"+data).hexdigest()

def edit(path, expected, transform):
    p=Path(path)
    raw=p.read_bytes()
    actual=blob_sha(raw)
    assert actual==expected, (path,actual,expected)
    p.write_text(transform(raw.decode()))

def core(text):
    for marker in [
        "    pub fn enqueue_content_update(\n",
        "    fn enqueue_operation(\n",
        "fn enqueue_operation_in_tx(\n",
    ]:
        assert text.count(marker)==1, marker
        indent=marker[:len(marker)-len(marker.lstrip())]
        text=text.replace(marker, indent+"#[allow(clippy::too_many_arguments)]\n"+marker, 1)
    old="    left.trim_end_matches(|ch| matches!(ch, '\\r' | '\\n'))\n        == right.trim_end_matches(|ch| matches!(ch, '\\r' | '\\n'))\n"
    new="    left.trim_end_matches(['\\r', '\\n']) == right.trim_end_matches(['\\r', '\\n'])\n"
    assert text.count(old)==1
    return text.replace(old,new)

def fixture(text):
    old="""#[derive(Default)]
struct SiYuanState {
    documents: Mutex<HashMap<String, String>>,
    hpaths: Mutex<HashMap<String, String>>,
    update_seen: Semaphore,
    update_release: Semaphore,
    fail_create_response_once: Mutex<bool>,
}
"""
    new="""struct SiYuanState {
    documents: Mutex<HashMap<String, String>>,
    hpaths: Mutex<HashMap<String, String>>,
    update_seen: Semaphore,
    update_release: Semaphore,
    fail_create_response_once: Mutex<bool>,
}

impl Default for SiYuanState {
    fn default() -> Self {
        Self {
            documents: Mutex::new(HashMap::new()),
            hpaths: Mutex::new(HashMap::new()),
            update_seen: Semaphore::new(0),
            update_release: Semaphore::new(0),
            fail_create_response_once: Mutex::new(false),
        }
    }
}
"""
    assert text.count(old)==1
    return text.replace(old,new)

edit("crates/aiks-core/src/team/content.rs","d4ca517d116c504afa8de99eed6230fb793ab076",core)
edit("apps/aiks-service/tests/team_content.rs","5ddaa8091bced34223682bd4679e70518bb782b7",fixture)
print("Applied guarded Task 8 compile fixes; branch is unchanged by this workflow.")
