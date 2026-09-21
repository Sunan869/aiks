import { describe, expect, it, vi } from "vitest";
import { runSearchRequest } from "./search-progress";

vi.mock("./client", () => ({ getApi: vi.fn(), shouldUseMock: () => false }));

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((ok, fail) => { resolve = ok; reject = fail; });
  return { promise, resolve, reject };
}

describe("progressive search transport", () => {
  it("publishes lexical hits before the final response and ignores late partials", async () => {
    const response = deferred<string>();
    let send!: (value: string) => void;
    const progress = vi.fn();
    const result = runSearchRequest((_id, callback) => {
      send = callback;
      return response.promise;
    }, async () => {}, progress);
    send("lexical");
    expect(progress).toHaveBeenCalledWith("lexical");
    response.resolve("hybrid");
    expect(await result).toBe("hybrid");
    send("stale lexical");
    expect(progress).toHaveBeenCalledTimes(1);
  });

  it("sends cancellation for the same request and ignores obsolete output", async () => {
    const response = deferred<string>();
    const controller = new AbortController();
    const cancel = vi.fn(async (_id: number) => {});
    const progress = vi.fn();
    let requestId = 0;
    let send!: (value: string) => void;
    const result = runSearchRequest((id, callback) => {
      requestId = id;
      send = callback;
      return response.promise;
    }, cancel, progress, controller.signal);
    const rejected = expect(result).rejects.toThrow("Search cancelled");
    controller.abort();
    send("obsolete");
    response.resolve("obsolete");
    await rejected;
    expect(cancel).toHaveBeenCalledWith(requestId);
    expect(progress).not.toHaveBeenCalled();
  });

  it("does not issue already-aborted work or cancel completed work", async () => {
    const controller = new AbortController();
    controller.abort();
    const start = vi.fn(async () => "done");
    const cancel = vi.fn(async () => {});
    await expect(runSearchRequest(start, cancel, undefined, controller.signal)).rejects.toThrow("Search cancelled");
    expect(start).not.toHaveBeenCalled();
    const active = new AbortController();
    expect(await runSearchRequest(start, cancel, undefined, active.signal)).toBe("done");
    active.abort();
    expect(cancel).not.toHaveBeenCalled();
  });

  it("does not erase previously emitted results when the final request fails", async () => {
    const progress = vi.fn();
    await expect(runSearchRequest(async (_id, publish) => {
      publish("lexical");
      throw new Error("connection lost");
    }, async () => {}, progress)).rejects.toThrow("connection lost");
    expect(progress.mock.calls).toEqual([["lexical"]]);
  });
});
