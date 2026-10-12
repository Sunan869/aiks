import { useState } from "react";
import { shouldUseMock } from "../api/client";
import { canConfirmIsolatedRestore, type OfflineRestorePreview } from "../offline-restore";

export function OfflineRestore() {
  const [bundle, setBundle] = useState("");
  const [destination, setDestination] = useState("");
  const [confirmed, setConfirmed] = useState("");
  const [preview, setPreview] = useState<OfflineRestorePreview | null>(null);
  const [previewDestination, setPreviewDestination] = useState("");
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const isMock = shouldUseMock();

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
      const result = await invoke<OfflineRestorePreview>("inspect_offline_backup", {
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
    <section className="rounded-lg border border-gray-200 bg-white p-4 dark:border-gray-700 dark:bg-gray-800">
      <h3 className="text-sm font-semibold mb-2">离线备份 · 安全隔离恢复</h3>
      <p className="text-xs text-amber-700 dark:text-amber-300 mb-3">
        此入口仅支持校验已有备份并恢复到全新独立目录，不覆盖当前工作空间。开始前请关闭 SiYuan 写入。
      </p>
      <label className="block text-xs mb-1" htmlFor="offline-bundle">备份目录（绝对路径）</label>
      <input id="offline-bundle" className="w-full rounded border p-2 text-xs mb-3 bg-transparent"
        value={bundle} disabled={busy} placeholder="例如 E:\\AIKS-Backup\\snapshot"
        onChange={e => { setBundle(e.target.value); clearPreview(); }} />
      <label className="block text-xs mb-1" htmlFor="offline-target">新恢复目录（必须不存在）</label>
      <input id="offline-target" className="w-full rounded border p-2 text-xs mb-3 bg-transparent"
        value={destination} disabled={busy} placeholder="例如 E:\\AIKS-Restore\\new-data"
        onChange={e => { setDestination(e.target.value); clearPreview(); }} />
      <button type="button" disabled={busy || !bundle.trim() || !destination.trim()} onClick={() => void inspect()}
        className="rounded bg-blue-600 px-3 py-2 text-white text-xs disabled:opacity-50">
        {busy ? "处理中…" : "校验备份并预览"}
      </button>
      {preview && (
        <div className="mt-3 rounded border border-gray-200 p-3 text-xs dark:border-gray-700">
          <p>文件数量：{preview.file_count}，SiYuan 文件：{preview.siyuan_files}，总大小：{preview.total_bytes.toLocaleString()} 字节</p>
          <p className="mt-2 text-amber-700 dark:text-amber-300">必须确认 SiYuan 不再写入。恢复不会激活或覆盖正在运行的工作空间。</p>
          <label className="block mt-3 mb-1" htmlFor="offline-confirm">再次输入完整恢复目标路径确认</label>
          <input id="offline-confirm" value={confirmed} disabled={busy}
            onChange={e => setConfirmed(e.target.value)}
            className="w-full rounded border p-2 bg-transparent" />
          <button type="button" onClick={() => void restore()}
            disabled={!canConfirmIsolatedRestore(bundle, destination, confirmed, previewDestination, preview, busy)}
            className="mt-3 rounded bg-amber-600 px-3 py-2 text-white disabled:opacity-50">
            确认恢复到全新目录
          </button>
        </div>
      )}
      {message && <p role="status" className="mt-3 text-xs break-words">{message}</p>}
    </section>
  );
}
