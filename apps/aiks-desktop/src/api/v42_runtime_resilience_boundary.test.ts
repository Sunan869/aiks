import { describe, expect, it } from "vitest";
import bootstrapSource from "../../../../crates/aiks-core/src/bootstrap/mod.rs?raw";
import runtimeSource from "../../../../crates/aiks-core/src/runtime/mod.rs?raw";
import syncEngineSource from "../../../../crates/aiks-core/src/sync/engine.rs?raw";
import lifecycleSource from "../../src-tauri/src/lifecycle/legacy.rs?raw";

describe("V4.2 SiYuan runtime resilience boundaries", () => {
  it("waits for the SiYuan kernel to finish booting instead of accepting version-only readiness", () => {
    expect(runtimeSource).toContain("/api/system/bootProgress");
    expect(runtimeSource).toContain("progress >= 100");
    expect(bootstrapSource).toContain("Duration::from_secs(120)");
  });

  it("detects a kernel process that exits after startup and monitors it from the desktop host", () => {
    expect(runtimeSource).toContain("try_wait()");
    expect(runtimeSource).toContain("SiYuan Kernel process exited unexpectedly after startup");
    expect(lifecycleSource).toContain("spawn_runtime_monitor");
    expect(lifecycleSource).toContain("runtime-unavailable");
  });

  it("resolves the session notebook once per sync run", () => {
    const runSyncStart = syncEngineSource.indexOf("pub async fn run_sync");
    const loopStart = syncEngineSource.indexOf("for summary in &summaries", runSyncStart);
    const ensureNotebook = syncEngineSource.indexOf("ensure_session_notebook().await", runSyncStart);

    expect(runSyncStart).toBeGreaterThanOrEqual(0);
    expect(loopStart).toBeGreaterThan(runSyncStart);
    expect(ensureNotebook).toBeGreaterThan(runSyncStart);
    expect(ensureNotebook).toBeLessThan(loopStart);
  });

  it("stops a batch after a failed write when SiYuan is no longer reachable", () => {
    expect(syncEngineSource).toContain("sink.health_check().await");
    expect(syncEngineSource).toContain("[SYNC] SiYuan unavailable; aborting remaining sessions");
  });
});
