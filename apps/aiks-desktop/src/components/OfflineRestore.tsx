import { UiButton, UiInput } from "./ui/fields";
import { useState } from "react";
import { shouldUseMock } from "../api/client";
import { canConfirmIsolatedRestore, type OfflineRestorePreview } from "../offline-restore";

export function OfflineRestore() {
  const [managedDestination, setManagedDestination] = useState("");
  const [managedConfirmed, setManagedConfirmed] = useState("");
  const [managedBusy, setManagedBusy] = useState(false);
  const [managedMessage, setManagedMessage] = useState("");
  const [recoveryBusy, setRecoveryBusy] = useState(false);
  const [recoveryMessage, setRecoveryMessage] = useState("");
  const [recoveryRequired, setRecoveryRequired] = useState<boolean | null>(null);
  const [bundle, setBundle] = useState("");
  const [destination, setDestination] = useState("");
  const [confirmed, setConfirmed] = useState("");
  const [preview, setPreview] = useState<OfflineRestorePreview | null>(null);
  const [previewDestination, setPreviewDestination] = useState("");
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const isMock = shouldUseMock();

  const createManaged = async () => {
    if (isMock || managedBusy || !managedDestination.trim() ||
      managedDestination.trim() !== managedConfirmed.trim()) return;
    setManagedBusy(true);
    setManagedMessage("");
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const count = await invoke<number>("create_managed_offline_backup", {
        destinationPath: managedDestination,
        confirmedDestinationPath: managedConfirmed,
      });
      setManagedMessage("当前工作空间完整备份完成，共 " + count + " 个文件。SiYuan 已恢复并通过健康检查。");
      setManagedConfirmed("");
    } catch (error) {
      setManagedMessage("当前工作空间备份失败：" + String(error) + "。如 SiYuan 无法恢复，请检查故障锁定状态。");
    } finally {
      setManagedBusy(false);
    }
  };

  const checkRecovery = async () => {
    if (isMock) {
      setRecoveryMessage("请在 AIKS 桌面应用中检查真实内核运行状态");
      return;
    }
    setRecoveryBusy(true);
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const required = await invoke<boolean>("get_managed_backup_recovery_required");
      setRecoveryRequired(required);
      setRecoveryMessage(required ? "检测到 SiYuan 恢复锁定。只有内核已恢复且健康检查通过，才能解除。" : "当前没有待解除的备份故障锁定。");
    } catch (error) {
      setRecoveryMessage("无法检查故障状态：" + String(error));
    } finally {
      setRecoveryBusy(false);
    }
  };

  const verifyRecovery = async () => {
    if (isMock || recoveryBusy || recoveryRequired !== true) return;
    setRecoveryBusy(true);
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke<boolean>("verify_managed_siyuan_recovery");
      setRecoveryRequired(false);
      setRecoveryMessage("内嵌 SiYuan 健康核验通过，已解除写入锁定。");
    } catch (error) {
      setRecoveryMessage("恢复核验失败，写入仍保持锁定：" + String(error));
    } finally {
      setRecoveryBusy(false);
    }
  };

  const clearPreview = () => {
    setPreview(null);
    setPreviewDestination("");
    setConfirmed("");
    setMessage("");
  };

  const inspect = async () => {
    clearPreview();
    if (isMock) {
      setMessage("浏览器预览模式不能校验真实备份，请使用 AIKS 桌面端");
      return;
    }
    setBusy(true);
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const result = await invoke<OfflineRestorePreview>("inspect_offline_backup_capacity", {
        bundlePath: bundle, destinationPath: destination,
      });
      setPreview(result);
      setPreviewDestination(destination);
      setMessage("备份预检成功。恢复只会生成一个全新隔离目录，不会修改当前 AIKS 数据。");
    } catch (error) {
      setMessage("预检失败：" + String(error));
    } finally {
      setBusy(false);
    }
  };

  const restore = async () => {
    if (!canConfirmIsolatedRestore(bundle, destination, confirmed, previewDestination, preview, busy)) return;
    setBusy(true);
    setMessage("");
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const count = await invoke<number>("restore_offline_backup_isolated", {
        bundlePath: bundle,
        destinationPath: destination,
        confirmedDestinationPath: confirmed,
      });
      setMessage("独立恢复完成，共复制 " + count + " 个文件。当前 AIKS 数据未切换。");
      clearAfterSuccess();
    } catch (error) {
      setMessage("恢复失败：" + String(error));
      setPreview(null);
      setConfirmed("");
    } finally {
      setBusy(false);
    }
  };

  const clearAfterSuccess = () => {
    setPreview(null);
    setPreviewDestination("");
    setConfirmed("");
  };

  return (
    <section className="aiks-surface min-w-0 space-y-4">
      <h3 className="text-base font-semibold">离线备份 · 安全隔离恢复</h3>
      <p className="text-xs text-amber-700 dark:text-amber-300 mb-3">
        此入口仅支持校验已有备份并恢复到全新独立目录，不覆盖当前工作空间。开始前请关闭 SiYuan 写入。
      </p>
      <div className="mb-4 border-b border-gray-200 pb-4 dark:border-gray-700">
        <h4 className="mb-2 text-xs font-semibold">当前 AIKS 工作空间备份（受控停写）</h4>
        <p className="mb-2 text-xs text-amber-700 dark:text-amber-300">此操作会暂时阻止后台写入并停止内嵌 SiYuan，备份完成后尝试重新启动；备份目标必须是数据目录之外的新目录。不要在备份过程中通过其他程序修改 SiYuan 工作空间。</p>
        <UiInput aria-label="当前工作空间备份目标" value={managedDestination}
          onChange={e => { setManagedDestination(e.target.value); setManagedConfirmed(""); }}
          disabled={managedBusy} placeholder="例如 E:\\AIKS-Backup\\snapshot-1"
          className="mb-2 w-full rounded border bg-transparent p-2 text-xs" />
        <UiInput aria-label="重新输入备份目标以确认" value={managedConfirmed}
          onChange={e => setManagedConfirmed(e.target.value)} disabled={managedBusy}
          placeholder="再次输入相同的新目录路径"
          className="mb-2 w-full rounded border bg-transparent p-2 text-xs" />
        <UiButton variant="outline" size="sm" type="button" disabled={isMock || managedBusy || !managedDestination.trim() ||
          managedDestination.trim() !== managedConfirmed.trim()} onClick={() => void createManaged()}
          className="border-amber-600 bg-amber-600 text-white hover:bg-amber-700">
          {managedBusy ? "正在停写、备份及重启…" : "确认创建当前工作空间备份"}
        </UiButton>
        {managedMessage && <p role="status" className="mt-2 break-words text-xs">{managedMessage}</p>}
      </div>
      <div className="mb-4 border-b border-gray-200 pb-4 dark:border-gray-700">
        <h4 className="mb-2 text-xs font-semibold">SiYuan 故障恢复核验</h4>
        <p className="mb-2 text-xs text-amber-700 dark:text-amber-300">仅当备份失败导致写入锁定时使用。此操作不会自动重启内核，也不能强制解锁。</p>
        <UiButton variant="outline" size="sm" type="button" disabled={recoveryBusy || isMock} onClick={() => void checkRecovery()}
          className="rounded border px-3 py-2 text-xs disabled:opacity-50">检查写入锁定状态</UiButton>
        {recoveryRequired === true && (
          <UiButton variant="outline" size="sm" type="button" disabled={recoveryBusy} onClick={() => void verifyRecovery()}
            className="ml-2 rounded bg-amber-600 px-3 py-2 text-xs text-white disabled:opacity-50">
            核验 SiYuan 健康并解除锁定
          </UiButton>
        )}
        {recoveryMessage && <p role="status" className="mt-2 break-words text-xs">{recoveryMessage}</p>}
      </div>
      <label className="block text-xs mb-1" htmlFor="offline-bundle">备份目录（绝对路径）</label>
      <UiInput id="offline-bundle" className="w-full rounded border p-2 text-xs mb-3 bg-transparent"
        value={bundle} disabled={busy} placeholder="例如 E:\\AIKS-Backup\\snapshot"
        onChange={e => { setBundle(e.target.value); clearPreview(); }} />
      <label className="block text-xs mb-1" htmlFor="offline-target">新恢复目录（必须不存在）</label>
      <UiInput id="offline-target" className="w-full rounded border p-2 text-xs mb-3 bg-transparent"
        value={destination} disabled={busy} placeholder="例如 E:\\AIKS-Restore\\new-data"
        onChange={e => { setDestination(e.target.value); clearPreview(); }} />
      <UiButton variant="outline" size="sm" type="button" disabled={busy || !bundle.trim() || !destination.trim()} onClick={() => void inspect()}
        className="border-sky-600 bg-sky-600 text-white hover:bg-sky-700">
        {busy ? "处理中…" : "校验备份并预览"}
      </UiButton>
      {preview && (
        <div className="mt-3 rounded border border-gray-200 p-3 text-xs dark:border-gray-700">
          <p>文件数量：{preview.file_count}，SiYuan 文件：{preview.siyuan_files}，总大小：{preview.total_bytes.toLocaleString()} 字节</p>
          <p className="mt-2">磁盘可用：{preview.available_bytes?.toLocaleString()} 字节；恢复预留需求：{preview.required_bytes?.toLocaleString()} 字节</p>
          <p className="mt-2 text-amber-700 dark:text-amber-300">必须确认 SiYuan 不再写入。恢复不会激活或覆盖正在运行的工作空间。</p>
          <label className="block mt-3 mb-1" htmlFor="offline-confirm">再次输入完整恢复目标路径确认</label>
          <UiInput id="offline-confirm" value={confirmed} disabled={busy}
            onChange={e => setConfirmed(e.target.value)}
            className="w-full rounded border p-2 bg-transparent" />
          <UiButton variant="outline" size="sm" type="button" onClick={() => void restore()}
            disabled={!canConfirmIsolatedRestore(bundle, destination, confirmed, previewDestination, preview, busy)}
            className="mt-3 rounded bg-amber-600 px-3 py-2 text-white disabled:opacity-50">
            确认恢复到全新目录
          </UiButton>
        </div>
      )}
      {message && <p role="status" className="mt-3 text-xs break-words">{message}</p>}
    </section>
  );
}
