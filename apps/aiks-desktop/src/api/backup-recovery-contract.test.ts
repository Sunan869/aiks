import { describe, expect, it } from "vitest";
import recoveryUI from "../components/OfflineRestore.tsx?raw";
import commands from "../../src-tauri/src/backup_commands.rs?raw";
import registrations from "../../src-tauri/src/lib.rs?raw";

describe("S1.3 verified recovery UI contract", () => {
  it("registers separate read-only status and verified unlock commands", () => {
    expect(registrations).toContain("backup_commands::get_managed_backup_recovery_required");
    expect(registrations).toContain("backup_commands::verify_managed_siyuan_recovery");
    expect(commands).toContain("pub fn get_managed_backup_recovery_required(");
    expect(commands).toContain("pub async fn verify_managed_siyuan_recovery(");
    expect(commands).toContain("health.state != RuntimeState::Ready");
  });

  it("only presents verified unlock after the backend reports a recovery latch", () => {
    expect(recoveryUI).toContain('invoke<boolean>("get_managed_backup_recovery_required")');
    expect(recoveryUI).toContain("recoveryRequired === true");
    expect(recoveryUI).toContain('invoke<boolean>("verify_managed_siyuan_recovery")');
    expect(recoveryUI).toContain("if (isMock || recoveryBusy || recoveryRequired !== true) return;");
  });
});
