// API singleton — returns Mock or Tauri implementation based on environment
import { shouldUseMock, type AiksApi } from "./index";
import { MockAiksApi } from "./mock";
import { TauriAiksApi } from "./tauri";

let _api: AiksApi | null = null;

export function getApi(): AiksApi {
  if (_api) return _api;
  _api = shouldUseMock() ? new MockAiksApi() : new TauriAiksApi();
  return _api;
}

export * from "./types";
export * from "./index";
