"""Bounded Windows rejection fix; no user data or ref changes."""
from pathlib import Path
import subprocess
p=Path('apps/aiks-service/src/routes.rs')
assert subprocess.check_output(['git','hash-object',str(p)],text=True).strip()=='647b0c93d434ec4bbf6e9e0a17594f37649de649'
s=p.read_text()
start=s.index('    if request\n        .headers()\n        .get(header::CONTENT_LENGTH)')
end=s.index('    let mut response = match',start)
s=s[:start]+'''    let Ok(_permit) = gate.inflight.try_acquire() else {
        return ApiError(ServiceError::Unavailable).into_response();
    };
    let length = request.headers().get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<u64>().ok());
    if length.is_some_and(|size| size > MAX_BODY as u64) {
        // On Windows, abandoning an in-flight body can reset the connection
        // before the client reads 413. Drain only a bounded near-limit body;
        // never deserialize it, wait unboundedly, or drain a huge declaration.
        const DRAIN_LIMIT: usize = MAX_BODY + 64 * 1024;
        if length.is_some_and(|size| size <= DRAIN_LIMIT as u64)
            && !request.headers().contains_key(header::EXPECT)
        {
            let _ = tokio::time::timeout(
                Duration::from_secs(2),
                axum::body::to_bytes(request.into_body(), DRAIN_LIMIT),
            ).await;
        }
        let mut response = ApiError(ServiceError::TooLarge).into_response();
        response.headers_mut().insert(header::CONNECTION, header::HeaderValue::from_static("close"));
        response.headers_mut().insert(header::CACHE_CONTROL, header::HeaderValue::from_static("no-store"));
        return response;
    }
''' + s[end:]
p.write_text(s)
p=Path('apps/aiks-service/tests/process_boundary.rs')
s=p.read_text()
s=s.replace('use serde_json::{json, Value};','#[cfg(unix)]\nuse serde_json::{json, Value};',1)
s=s.replace('use tokio::{\n    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},\n    process::Command,\n};','use tokio::process::Command;\n#[cfg(unix)]\nuse tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};',1)
p.write_text(s)
