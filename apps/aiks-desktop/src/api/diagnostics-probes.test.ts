import { describe, expect, it } from "vitest";
import { collectDiagnostics } from "../diagnostics-probes";

describe("independent desktop diagnostics", () => {
  it("keeps successful health checks when a different service throws", async () => {
    const snapshot = await collectDiagnostics({
      doctor: async () => ({ checks: [{ name: "State DB", ok: true, message: "OK" }] }),
      ai: async (): Promise<boolean> => { throw new Error("token=private-secret"); },
      workbench: async () => ({ siyuan_ready: true }),
    });
    expect(snapshot.doctor).toEqual({ status: "ok", value: { checks: [{ name: "State DB", ok: true, message: "OK" }] } });
    expect(snapshot.workbench).toEqual({ status: "ok", value: { siyuan_ready: true } });
    expect(snapshot.failures).toEqual(["ai"]);
    expect(JSON.stringify(snapshot)).not.toContain("private-secret");
  });

  it("distinguishes a completed negative AI health check from a thrown failure", async () => {
    const snapshot = await collectDiagnostics({
      doctor: async () => ({ all_ok: false }),
      ai: async () => false,
      workbench: async () => ({ siyuan_ready: false }),
    });
    expect(snapshot.ai).toEqual({ status: "ok", value: false });
    expect(snapshot.failures).toEqual([]);
  });

  it("isolates synchronous errors and concurrent late completion", async () => {
    let release!: (value: number) => void;
    const workbench = new Promise<number>(resolve => { release = resolve; });
    const pending = collectDiagnostics({
      doctor: () => { throw new Error("local private path"); },
      ai: async () => 1,
      workbench: () => workbench,
    });
    release(4);
    const snapshot = await pending;
    expect(snapshot.doctor).toEqual({ status: "error" });
    expect(snapshot.ai).toEqual({ status: "ok", value: 1 });
    expect(snapshot.workbench).toEqual({ status: "ok", value: 4 });
    expect(snapshot.failures).toEqual(["doctor"]);
    expect(JSON.stringify(snapshot)).not.toContain("local private path");
  });

  it("returns three failures without raw service details", async () => {
    const broken = () => Promise.reject(new Error("Authorization: Bearer private"));
    const snapshot = await collectDiagnostics({ doctor: broken, ai: broken, workbench: broken });
    expect(snapshot.failures).toEqual(["doctor", "ai", "workbench"]);
    expect(JSON.stringify(snapshot)).not.toContain("Bearer");
  });
});
