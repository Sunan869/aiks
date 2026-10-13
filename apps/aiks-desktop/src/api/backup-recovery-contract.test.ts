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
  it("binds managed backup to the active workspace and requires two matching paths", () => {
    expect(registrations).toContain("backup_commands::create_managed_offline_backup");
    expect(commands).toContain("pub async fn create_managed_offline_backup(");
    expect(commands).toContain('let db_path = root.join("aiks.db");');
    expect(commands).toContain('let workspace = root.join("siyuan").join("workspace");');
    expect(commands).toContain("validate_managed_backup_paths(&root, &db_path, &workspace, &destination)?;");
    expect(commands).toContain("destination_path.trim() != confirmed_destination_path.trim()");
    expect(commands).toContain("available_restore_bytes(&destination)?");
    expect(commands).toContain("create_managed_quiesced_backup(");
    expect(recoveryUI).toContain('invoke<number>("create_managed_offline_backup"');
    expect(recoveryUI).toContain("managedDestination.trim() !== managedConfirmed.trim()");
  });
});
