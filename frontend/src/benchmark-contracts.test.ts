import { afterEach, expect, it, vi } from "vitest";
import { api, ApiProblem } from "./api";

afterEach(() => vi.unstubAllGlobals());
it("cancels Benchmark reads and encodes Run identity without sending invented contract fields", async () => {
  const controller = new AbortController();
  const requests: [string, RequestInit | undefined][] = [];
  vi.stubGlobal("fetch", async (path: string, init?: RequestInit) => {
    requests.push([path, init]);
    return new Response(JSON.stringify({}));
  });
  await api.getBenchmarkRun("run /1", controller.signal);
  await api.listBenchmarkRuns(controller.signal);
  await api.getAdminRoom("123456", controller.signal);
  await api.listAdminReplays(0, 50, controller.signal);
  expect(requests.map(([path]) => path)).toEqual(["/api/v1/admin/benchmark/runs/run%20%2F1", "/api/v1/admin/benchmark/runs", "/api/v1/admin/rooms/123456", "/api/v1/admin/replays"]);
  expect(requests.every(([, init]) => init?.signal === controller.signal && init?.credentials === "include")).toBe(true);
  await api.stopBenchmarkRun("123456");
  expect(requests[4][1]?.method).toBe("POST");
  expect(requests[4][1]?.body).toBeUndefined();
});
it.each([0, 1001, 1.5])("rejects target %s before transport", target => {
  vi.stubGlobal("fetch", vi.fn());
  expect(() => api.startBenchmarkRun("123456", target)).toThrow(ApiProblem);
  expect(fetch).not.toHaveBeenCalled();
});
it("preserves authorization and conflict failures without automatic transport retries", async () => {
  for (const status of [401, 409, 503]) {
    vi.stubGlobal("fetch", vi.fn(async () => new Response(JSON.stringify({ detail: "Rejected" }), { status })));
    await expect(api.stopBenchmarkRun("123456")).rejects.toMatchObject({ status });
    expect(fetch).toHaveBeenCalledTimes(1);
  }
});
it("preserves cancellation rather than converting it to a host failure", async () => {
  const controller = new AbortController();
  controller.abort();
  vi.stubGlobal("fetch", async () => { throw new DOMException("Aborted", "AbortError"); });
  await expect(api.getBenchmarkRun("run-1", controller.signal)).rejects.toMatchObject({ name: "AbortError" });
});
