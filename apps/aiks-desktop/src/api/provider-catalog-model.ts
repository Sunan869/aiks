export interface SourceDescriptor {
  key: string;
  display_name: string;
  config_key: string;
  configurable?: boolean;
  enabled: boolean;
  paths: string[];
  status: string;
  message: string;
  restart_required?: boolean;
}

export function sourceStateLabel(source: SourceDescriptor): string {
  if (source.restart_required) return "待重启生效";
  if (!source.enabled) return "已禁用";
  return ({ ok: "可读取", not_found: "未检测到", not_configured: "未配置目录", unsupported: "格式不支持", error: "读取异常", unknown: "状态未知" } as Record<string, string>)[source.status] ?? "状态未知";
}

export function sourceFilterOptions(sources: readonly SourceDescriptor[]): Array<{ value: string; label: string }> {
  return sources.map(source => ({ value: source.key, label: source.display_name }));
}

export async function syncCatalogSource<T>(api: { syncAndExtract: (source: string) => Promise<T> }, source: SourceDescriptor): Promise<T> {
  if (!source.enabled || source.restart_required) throw new Error("请启用数据源并重启 AIKS 后再同步");
  return api.syncAndExtract(source.key);
}
