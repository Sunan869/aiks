import { describe, expect, it, vi } from "vitest";
import { ServiceApi, statusText } from "./service";

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
