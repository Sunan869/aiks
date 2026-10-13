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

export type SourceStatusTone = "success" | "neutral" | "warning" | "danger";

export interface SourceStatusPresentation {
  label: string;
  title: string | null;
  description: string | null;
  tone: SourceStatusTone;
}

export function sourceStatusPresentation(source: SourceDescriptor): SourceStatusPresentation {
  if (source.restart_required) {
    return {
      label: "待重启生效",
      title: "配置将在重启后生效",
      description: "当前进程仍使用旧配置，重启 AIKS 后会按新设置扫描。",
      tone: "warning",
    };
  }
  if (!source.enabled) {
    return {
      label: "已禁用",
      title: null,
      description: null,
      tone: "neutral",
    };
  }

  switch (source.status) {
    case "ok":
      return {
        label: "可读取",
        title: null,
        description: null,
        tone: "success",
      };
    case "not_found":
      return {
        label: "未检测到",
        title: "未检测到本地会话数据",
        description: "未找到该工具的本地会话目录。如果已经使用过该工具，请检查「目录与开关」中的路径配置。",
        tone: "warning",
      };
    case "not_configured":
      return {
        label: "未配置",
        title: "尚未配置数据目录",
        description: "配置数据目录后，AIKS 将自动扫描该工具的本地会话记录。",
        tone: "warning",
      };
    case "unsupported":
      return {
        label: "读取异常",
        title: "本地会话数据读取失败",
        description: "检测到本地会话数据，但当前数据格式暂不支持读取。",
        tone: "danger",
      };
    case "error":
    case "unknown":
    default:
      return {
        label: "读取异常",
        title: "本地会话数据读取失败",
        description: "请检查数据目录、访问权限或「目录与开关」中的配置。",
        tone: "danger",
      };
  }
}

export function sourceStateLabel(source: SourceDescriptor): string {
  return sourceStatusPresentation(source).label;
}

export function sourceFilterOptions(sources: readonly SourceDescriptor[]): Array<{ value: string; label: string }> {
  return sources.map(source => ({ value: source.key, label: source.display_name }));
}

export async function syncCatalogSource<T>(api: { syncAndExtract: (source: string) => Promise<T> }, source: SourceDescriptor): Promise<T> {
  if (!source.enabled || source.restart_required) throw new Error("请启用数据源并重启 AIKS 后再同步");
  return api.syncAndExtract(source.key);
}
