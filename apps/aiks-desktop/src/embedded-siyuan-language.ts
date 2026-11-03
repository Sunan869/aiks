import type { UiLanguage } from "./i18n";

export interface EmbeddedLanguageOps {
  get: () => Promise<UiLanguage | null>;
  set: (language: UiLanguage) => Promise<boolean>;
  reload: () => Promise<void>;
  waitReady: () => Promise<boolean>;
}

/**
 * Synchronize only SiYuan's *interface* language. This never translates or
 * writes provider source content, knowledge records, or SiYuan documents.
 * The caller should invoke this after the embedded Bridge is ready.
 */
export async function synchronizeEmbeddedLanguage(
  desired: UiLanguage,
  operations: EmbeddedLanguageOps,
): Promise<boolean> {
  const current = await operations.get();
  if (current === desired) return false;
  const changed = await operations.set(desired);
  if (!changed) return false;
  await operations.reload();
  if (!(await operations.waitReady())) {
    throw new Error("SiYuan language changed, but the embedded workbench did not become ready after reload");
  }
  return true;
}
