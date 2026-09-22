/** OS/window actions stay separate from the knowledge-service API. */
export const desktopPlatformApi = {
  async restart(): Promise<void> {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("restart_app");
  },
};
