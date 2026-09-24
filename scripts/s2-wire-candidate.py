"""SHA-guarded S2 Task 6/7 core candidate. CI formats and emits blobs; it never moves a ref."""
from pathlib import Path
import base64,gzip,hashlib,subprocess
EXPECTED = {'crates/aiks-core/src/team/sessions.rs': '38c0264a9094ea674b7300cc755da68e6ce8fad8', 'crates/aiks-core/src/team/auth_types.rs': '9718f27fea1077745d2b4491e7132c4542032c96', 'crates/aiks-core/src/team/mod.rs': '071227b6efaa21ccedf44f71be3f8327a6cd9579', 'crates/aiks-core/src/team/shares.rs': None, 'crates/aiks-core/src/search/scope.rs': 'cc3728807c543428426ef31bed80dcf210538dce', 'crates/aiks-core/src/search/lexical.rs': '915356033e280769014b24ab05691951d02ffa44', 'crates/aiks-core/src/search/mod.rs': '944918ce5c9b392a485dac5f7a6b9db28949a1a2', 'crates/aiks-core/src/service/ingestion.rs': '371028cfe5cf5a6cfa66f4790df95c5afe8ad463', 'crates/aiks-core/src/service/query.rs': '62c8bf2bcd7f60095f896623f925a3fed9d9b492', 'crates/aiks-core/src/service/repo.rs': '6db82d2ef2e9f0b5d34b507bf965892a12a0214d', 'crates/aiks-core/src/service/contracts.rs': 'd00485a50d078af329231aaa026b28f8f7a719c7', 'crates/aiks-core/src/service/mod.rs': '92fe9050183d35ca02f554554c38716ec2bcc58e', 'crates/aiks-core/src/service/runtime.rs': 'aab585a9de227a92bee7322e2305da8c3216f313', 'crates/aiks-core/src/pipeline/knowledge_repo.rs': '0045a3d60713692478e40dd182117eac9f4f98cb', 'crates/aiks-core/tests/team_sharing.rs': None, 'apps/aiks-service/src/error.rs': '8685d3c1c6d147d911c65f26847c4d3b934fe723'}
PATCH_SHA256 = '94c7e2ee2cc28ea3fbd32e31e715516191cd5b10a03757de2ebe90eec353857d'
for path, expected in EXPECTED.items():
    p=Path(path)
    if expected is None:
        assert not p.exists(), (path, 'unexpected-existing-file')
    else:
        actual=subprocess.check_output(['git','hash-object',path],text=True).strip()
        assert actual==expected, (path,actual,expected)
encoded=''.join(Path(f'scripts/s2-task67-core-patch-{i}.txt').read_text() for i in range(3))
patch=gzip.decompress(base64.b64decode(encoded))
assert hashlib.sha256(patch).hexdigest()==PATCH_SHA256
subprocess.run(['git','apply','--whitespace=nowarn','-'],input=patch,check=True)
print('Applied SHA-guarded Task 6/7 core candidate; branch still requires explicit reviewed commit.')
