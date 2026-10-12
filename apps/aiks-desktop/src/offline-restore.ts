export interface OfflineRestorePreview {
  file_count: number;
  total_bytes: number;
  siyuan_files: number;
  requires_siyuan_shutdown: boolean;
}

export function canConfirmIsolatedRestore(
  bundle: string,
  destination: string,
  confirmation: string,
  previewDestination: string,
  inspected: OfflineRestorePreview | null,
  busy: boolean,
): boolean {
  return !busy && inspected !== null
    && bundle.trim().length > 0 && destination.trim().length > 0
    && destination === previewDestination
    && confirmation === destination;
}
