import { invoke } from "@tauri-apps/api/core";
import { shouldUseMock } from "./client";
import type { SourceDescriptor } from "./provider-catalog-model";
import { mockSourceDescriptors } from "./provider-catalog.mock";

let mockSources = mockSourceDescriptors();

export async function getSourceDescriptors(): Promise<SourceDescriptor[]> {
  const sources = shouldUseMock() ? mockSources.map(source => ({ ...source, paths: [...source.paths] })) : await invoke<SourceDescriptor[]>("get_source_descriptors");
  if (!Array.isArray(sources) || !sources.every(s => typeof s.key === "string" && typeof s.display_name === "string" && typeof s.enabled === "boolean" && Array.isArray(s.paths))) {
    throw new Error("数据源目录返回了无效结构");
  }
  if (new Set(sources.map(s => s.key)).size !== sources.length) throw new Error("数据源目录包含重复标识");
  return sources;
}

export async function saveProviderSettings(source: string, enabled: boolean, paths: string[]): Promise<void> {
  if (shouldUseMock()) {
    if (!mockSources.some(s => s.key === source)) throw new Error("未知数据源");
    mockSources = mockSources.map(s => s.key === source ? { ...s, enabled, paths: [...paths], restart_required: true } : s);
    return;
  }
  await invoke("save_provider_settings", { source, enabled, paths });
}
