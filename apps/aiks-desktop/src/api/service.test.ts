import { describe, expect, it, vi } from "vitest";
import { ServiceApi, isServiceWorkspaceMode, statusText } from "./service";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import ServiceStatusPage from "../pages/ServiceStatusPage";
import Tasks from "../pages/service/Tasks";
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


describe("remote desktop startup mode", () => {
  it("accepts the native service_remote status and selects the Service workspace", async () => {
    const raw = {mode:"service_remote", phase:"waiting_for_login"};
    const api = new ServiceApi(vi.fn().mockResolvedValue(raw));
    const status = await api.status();
    expect(status).toEqual(raw);
    expect(isServiceWorkspaceMode(status.mode)).toBe(true);
    expect(isServiceWorkspaceMode("service_local")).toBe(true);
    expect(isServiceWorkspaceMode("legacy")).toBe(false);
  });
  it("still rejects unsupported native modes", async () => {
    const api = new ServiceApi(vi.fn().mockResolvedValue({mode:"unknown", phase:"ready"}));
    await expect(api.status()).rejects.toThrow("invalid_service_response");
  });
});


describe("collector delivery reconciliation", () => {
  it("keeps upstream Session identity with the acknowledged upload row", async () => {
    const api = new ServiceApi(vi.fn().mockResolvedValue([{
      id:"upload-1",state:"acknowledged",source:"opencode",
      external_session_id:"opencode-session-1",attempt:1,error_code:null,receipt:null
    }]));
    const uploads=await api.uploads();
    expect(uploads[0].external_session_id).toBe("opencode-session-1");
    expect(uploads[0].state).toBe("acknowledged");
  });
  it("validates explicit retry counts", async () => {
    const invoke=vi.fn().mockResolvedValueOnce({queued:1}).mockResolvedValueOnce({queued:-1});
    const api=new ServiceApi(invoke);
    expect(await api.retryWeKnoraFailed()).toBe(1);
    await expect(api.retryWeKnoraFailed()).rejects.toThrow("invalid_service_response");
    expect(invoke.mock.calls.map(call=>call[0])).toEqual([
      "service_retry_weknora_failed","service_retry_weknora_failed"
    ]);
  });
});


describe("remote delivery UI stages", () => {
  it("shows WeKnora creation counts without claiming parsing is complete", () => {
    const html=renderToStaticMarkup(createElement(Tasks,{
      ready:true,
      status:{
        mode:"service_remote",phase:"ready",
        weknora:{enabled:true,available:true,delivered:99,pending:0,terminal:1,failures:[],recent:[]}
      }
    }));
    expect(html).toContain("WeKnora 已返回知识 ID");
    expect(html).toContain("99");
    expect(html).toContain("不代表知识解析");
    expect(html).toContain("重试前 10 条");
  });
  it("never turns unavailable remote status into a false zero", () => {
    const html=renderToStaticMarkup(createElement(Tasks,{
      ready:true,
      status:{
        mode:"service_remote",phase:"ready",
        weknora:{enabled:true,available:false,error_code:"collector_upgrade_required"}
      }
    }));
    expect(html).toContain("暂时无法查询投递状态");
    expect(html).toContain("collector_upgrade_required");
  });
});


describe("offline remote delivery UI", () => {
  it("does not hide local durable uploads just because remote health is unavailable", () => {
    const html=renderToStaticMarkup(createElement(Tasks,{
      ready:false,
      status:{mode:"service_remote",phase:"unavailable",
        weknora:{enabled:true,available:false,error_code:"status_timeout"}}
    }));
    expect(html).toContain("正在读取本地上传记录");
    expect(html).toContain("暂时无法查询投递状态");
  });
});
