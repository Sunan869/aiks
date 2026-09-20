import { useCallback, useEffect, useState } from "react";
import { FolderOpen, HardDrive, MoveRight } from "lucide-react";
import { shouldUseMock } from "../api/client";

interface DataStorageSettings {
  current_root: string;
  default_root: string;
  setup_required: boolean;
  custom: boolean;
  env_override: boolean;
  migration_note?: string | null;
}

async function getStorageSettings(): Promise<DataStorageSettings> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<DataStorageSettings>("get_data_storage_settings");
}

async function chooseDirectory(): Promise<string | null> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<string | null>("pick_data_directory");
}

async function applyDirectory(path: string): Promise<void> {
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("set_data_storage_root", { targetPath: path });
}

export function DataStorageSetupGate({ onRequired }: { onRequired: () => void }) {
  const isMock = shouldUseMock();
  const [settings, setSettings] = useState<DataStorageSettings | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    if (isMock) return;
    let disposed = false;
    void getStorageSettings()
      .then(value => {
        if (disposed) return;
        setSettings(value);
        if (value.setup_required) onRequired();
      })
      .catch(() => {});
    return () => { disposed = true; };
  }, [isMock, onRequired]);

  const selectCustom = useCallback(async () => {
    setError("");
    try {
      const selected = await chooseDirectory();
      if (!selected) return;
      setBusy(true);
      await applyDirectory(selected);
    } catch (e) {
      setBusy(false);
      setError(String(e));
    }
  }, []);

  const useDefault = useCallback(async () => {
    if (!settings) return;
    setBusy(true);
    setError("");
    try {
      await applyDirectory(settings.default_root);
    } catch (e) {
      setBusy(false);
      setError(String(e));
    }
  }, [settings]);

  if (isMock || !settings?.setup_required) return null;

  return (
    <div className="fixed inset-0 z-[100] flex items-center justify-center bg-black/50 p-6 backdrop-blur-sm">
      <div className="w-full max-w-xl rounded-xl border border-gray-200 bg-white p-6 shadow-2xl dark:border-gray-700 dark:bg-gray-800">
        <div className="flex items-start gap-3">
          <div className="rounded-lg bg-blue-50 p-2 text-blue-600 dark:bg-blue-900/30 dark:text-blue-300">
            <HardDrive size={22} />
          </div>
          <div>
            <h2 className="text-lg font-semibold">选择 AIKS 数据存储位置</h2>
            <p className="mt-1 text-sm leading-6 text-gray-500 dark:text-gray-400">
              AIKS 会在这里保存数据库、SiYuan 工作区、索引、归档和日志。建议数据较多时选择空间充足的磁盘。
            </p>
          </div>
        </div>

        <div className="mt-5 rounded-lg border border-gray-200 bg-gray-50 p-3 dark:border-gray-700 dark:bg-gray-900/40">
          <div className="text-[11px] uppercase tracking-wide text-gray-400">默认位置</div>
          <div className="mt-1 break-all font-mono text-xs text-gray-700 dark:text-gray-300">{settings.default_root}</div>
        </div>

        {error && <div className="mt-4 rounded border border-red-200 bg-red-50 px-3 py-2 text-xs text-red-600 dark:border-red-900/60 dark:bg-red-950/20 dark:text-red-300">{error}</div>}

        <div className="mt-6 flex flex-wrap justify-end gap-2">
          <button
            type="button"
            disabled={busy}
            onClick={() => void useDefault()}
            className="rounded border border-gray-300 px-4 py-2 text-sm hover:bg-gray-50 disabled:opacity-50 dark:border-gray-600 dark:hover:bg-gray-700"
          >
            使用默认位置
          </button>
          <button
            type="button"
            disabled={busy}
            onClick={() => void selectCustom()}
            className="flex items-center gap-2 rounded bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-700 disabled:opacity-50"
          >
            <FolderOpen size={16} />
            {busy ? "正在应用..." : "选择其他目录"}
          </button>
        </div>
        <div className="mt-3 text-right text-[11px] text-gray-400">保存后 AIKS 会自动重启并使用新目录。</div>
      </div>
    </div>
  );
}

export function DataStorageSettingsSection() {
  const isMock = shouldUseMock();
  const [settings, setSettings] = useState<DataStorageSettings | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  const refresh = useCallback(() => {
    if (isMock) return;
    void getStorageSettings().then(setSettings).catch(e => setError(String(e)));
  }, [isMock]);

  useEffect(() => { refresh(); }, [refresh]);

  const changeLocation = async () => {
    if (!settings || settings.env_override) return;
    setError("");
    try {
      const selected = await chooseDirectory();
      if (!selected || selected === settings.current_root) return;
      const confirmed = window.confirm(
        `将 AIKS 数据从：\n${settings.current_root}\n\n迁移到：\n${selected}\n\nAIKS 会立即重启，并在打开数据库和 SiYuan 之前完成复制与校验。原目录会暂时保留作为安全备份。是否继续？`,
      );
      if (!confirmed) return;
      setBusy(true);
      await applyDirectory(selected);
    } catch (e) {
      setBusy(false);
      setError(String(e));
    }
  };

  const openFolder = async () => {
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("open_data_folder");
    } catch (e) {
      setError(String(e));
    }
  };

  if (isMock) return null;

  return (
    <div className="px-6 pb-8">
      <div className="border-t border-gray-200 pt-6 dark:border-gray-700">
        <div className="mb-3 flex items-center gap-2">
          <HardDrive size={17} className="text-gray-500" />
          <h2 className="text-sm font-semibold">数据存储位置</h2>
        </div>
        <div className="rounded-lg border border-gray-200 bg-white p-4 dark:border-gray-700 dark:bg-gray-800">
          {!settings ? (
            <div className="text-xs text-gray-400">正在读取数据目录...</div>
          ) : (
            <>
              <div className="flex flex-wrap items-start justify-between gap-4">
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2 text-xs text-gray-400">
                    当前目录
                    <span className="rounded bg-gray-100 px-1.5 py-0.5 text-[10px] text-gray-500 dark:bg-gray-700 dark:text-gray-300">
                      {settings.env_override ? "环境变量" : settings.custom ? "自定义" : "默认"}
                    </span>
                  </div>
                  <div className="mt-1 break-all font-mono text-xs text-gray-700 dark:text-gray-300">{settings.current_root}</div>
                  <div className="mt-2 text-[11px] leading-5 text-gray-400">
                    数据目录包含 aiks.db、SiYuan workspace、索引、归档与日志。修改位置时 AIKS 会先重启，再在数据库打开前迁移并校验数据。
                  </div>
                </div>
                <div className="flex shrink-0 flex-wrap gap-2">
                  <button
                    type="button"
                    onClick={() => void openFolder()}
                    className="flex items-center gap-1.5 rounded border border-gray-300 px-3 py-1.5 text-xs hover:bg-gray-50 dark:border-gray-600 dark:hover:bg-gray-700"
                  >
                    <FolderOpen size={14} /> 打开目录
                  </button>
                  <button
                    type="button"
                    disabled={busy || settings.env_override}
                    onClick={() => void changeLocation()}
                    className="flex items-center gap-1.5 rounded bg-blue-600 px-3 py-1.5 text-xs text-white hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-50"
                  >
                    <MoveRight size={14} /> {busy ? "正在重启..." : "更改位置"}
                  </button>
                </div>
              </div>

              {settings.env_override && (
                <div className="mt-3 rounded border border-yellow-200 bg-yellow-50 px-3 py-2 text-xs text-yellow-700 dark:border-yellow-900/50 dark:bg-yellow-950/20 dark:text-yellow-300">
                  当前目录由 AIKS_DATA_DIR 环境变量控制。为避免配置冲突，界面内迁移已禁用。
                </div>
              )}
              {settings.migration_note && (
                <div className="mt-3 rounded border border-blue-100 bg-blue-50 px-3 py-2 text-xs leading-5 text-blue-700 dark:border-blue-900/40 dark:bg-blue-950/20 dark:text-blue-300">
                  {settings.migration_note}
                </div>
              )}
              {error && <div className="mt-3 rounded border border-red-200 bg-red-50 px-3 py-2 text-xs text-red-600 dark:border-red-900/60 dark:bg-red-950/20 dark:text-red-300">{error}</div>}
            </>
          )}
        </div>
      </div>
    </div>
  );
}
