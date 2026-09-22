import { describe, expect, it, vi } from "vitest";
import { ServiceApi, statusText } from "./service";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import ServiceStatusPage from "../pages/ServiceStatusPage";
import lifecycle from "../../src-tauri/src/lifecycle.rs?raw";
import controller from "../../src-tauri/src/service_desktop.rs?raw";
import dev from "../../../../scripts/dev.ps1?raw";

describe("service desktop flow", () => {
  it("does not describe accepted or failed work as extraction success", () => {
    expect(statusText({receiptState:"accepted",jobState:"PENDING"})).toBe("已接收，等待处理");
    expect(statusText({receiptState:"accepted",jobState:"FAILED"})).toBe("处理失败");
    expect(statusText({receiptState:"accepted",jobState:"SUPERSEDED"})).toBe("已被新版本替代");
  });
  it("calls only controlled native actions and never falls back after an error", async () => {
    const invoke=vi.fn().mockRejectedValueOnce("unavailable").mockResolvedValueOnce({hits:[],degraded:false,warnings:[]});
    const api=new ServiceApi(invoke);
    await expect(api.search("why")).rejects.toThrow();
    expect((await api.search("why")).hits).toEqual([]);
    expect(invoke.mock.calls.map(v=>v[0])).toEqual(["service_search","service_search"]);
  });
  it("rejects invalid server responses rather than claiming an empty search", async () => {
    const api=new ServiceApi(vi.fn().mockResolvedValue({unexpected:true}));
    await expect(api.search("why")).rejects.toThrow();
  });
});
describe("service mode is a separate complete startup path", () => {
  it("does not initialize legacy engines or expose the content origin", () => {
    expect(controller).not.toContain("AiksEngine::initialize");
    expect(controller).not.toContain("PipelineWorker::start");
    expect(lifecycle).toContain("BackendMode::ServiceLocal");
    expect(lifecycle).toContain("BusinessDbLease::acquire");
    expect(dev).toContain("cargo build --locked -p aiks-service");
    expect(dev).toContain('$env:AIKS_BACKEND_MODE = "service_local"');
  });
  it("renders the real Service page without claiming unobserved completion", () => {
    const html=renderToStaticMarkup(createElement(ServiceStatusPage));
    expect(html).toContain("本地知识服务");
    expect(html).toContain("未启用");
    expect(html).not.toContain("处理完成");
  });
});
