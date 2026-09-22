import {describe,expect,it} from "vitest";
import controller from "../../src-tauri/src/service_desktop.rs?raw";
import tray from "../../src-tauri/src/tray.rs?raw";
import page from "../pages/ServiceStatusPage.tsx?raw";

describe("service startup does not touch a legacy personal workspace",()=>{
  it("uses the read-only runtime locator instead of the legacy plugin installer",()=>{
    expect(controller).toContain("locate_runtime_root(app)");
    expect(controller).not.toContain("find_runtime_root(app)");
  });
  it("serializes process startup with cancellation and cleanup",()=>{
    expect(controller).toContain("lifecycle_gate");
    expect(controller).toContain("ensure_running()");
  });
  it("keeps service tray actions in the service control shell",()=>{
    expect(tray).toContain("ServiceDesktop");
    expect(tray).toContain("service_mode");
  });
  it("does not promise local-only inference for configurable model endpoints",()=>{
    expect(page).not.toContain("数据只在本机");
    expect(page).toContain("模型配置");
  });
});
