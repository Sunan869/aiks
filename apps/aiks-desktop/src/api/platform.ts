/** OS/window actions deliberately remain separate from the business service API. */
export const desktopPlatformApi={
  async openDataFolder():Promise<void>{const {invoke}=await import("@tauri-apps/api/core");await invoke("open_data_folder");},
  async restart():Promise<void>{const {invoke}=await import("@tauri-apps/api/core");await invoke("restart_app");},
};
