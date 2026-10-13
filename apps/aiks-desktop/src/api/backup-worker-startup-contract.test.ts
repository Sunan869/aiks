import { describe, expect, it } from "vitest";
import engine from "../../../../crates/aiks-core/src/engine/mod.rs?raw";
import worker from "../../../../crates/aiks-core/src/pipeline/worker.rs?raw";
import lifecycle from "../../src-tauri/src/lifecycle.rs?raw";

describe("S1.3 Worker quiescence activation order", () => {
  it("initializes worker gate before spawn rather than depending on later injection", () => {
    const creation = worker.indexOf("let backup_gate = Arc::new(RwLock::new(initial_gate));");
    const spawning = worker.indexOf("tokio::spawn(async move", creation);
    expect(creation).toBeGreaterThan(-1);
    expect(spawning).toBeGreaterThan(creation);
    expect(engine).toContain("PipelineWorker::start_with_gate(");
    expect(engine).toContain("backup_gate,");
  });

  it("passes shared backup gate on all desktop engine initialization paths", () => {
    const matches = lifecycle.match(/AiksEngine::initialize_with_backup_gate\(/g) ?? [];
    expect(matches).toHaveLength(3);
    expect(lifecycle).not.toContain("engine.set_backup_gate(");
  });
});
