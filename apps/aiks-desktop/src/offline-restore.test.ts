import { describe, expect, it } from "vitest";
import { canConfirmIsolatedRestore, type OfflineRestorePreview } from "./offline-restore";

const preview: OfflineRestorePreview = {
  file_count: 3, total_bytes: 2048, siyuan_files: 1, requires_siyuan_shutdown: true,
};

describe("isolated restore confirmation safety", () => {
  it("requires exact confirmation for the previewed destination", () => {
    expect(canConfirmIsolatedRestore("B", "D", "D", "D", preview, false)).toBe(true);
    expect(canConfirmIsolatedRestore("B", "D", "d", "D", preview, false)).toBe(false);
    expect(canConfirmIsolatedRestore("B", "D", "D", "old", preview, false)).toBe(false);
  });
  it("blocks no preview, empty paths, and concurrent operations", () => {
    expect(canConfirmIsolatedRestore("B", "D", "D", "D", null, false)).toBe(false);
    expect(canConfirmIsolatedRestore("", "D", "D", "D", preview, false)).toBe(false);
    expect(canConfirmIsolatedRestore("B", "", "", "", preview, false)).toBe(false);
    expect(canConfirmIsolatedRestore("B", "D", "D", "D", preview, true)).toBe(false);
  });
});
