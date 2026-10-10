// @vitest-environment node
import { afterEach, beforeEach, expect, it, vi, type MockInstance } from "vitest";
import { spawn, type ChildProcess } from "node:child_process";
import { EventEmitter } from "node:events";
import { PassThrough } from "node:stream";
import { mkdtempSync, readFileSync, readdirSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { performance } from "node:perf_hooks";
import { runCommand } from "../../tests/support/real-server";

vi.mock(import("node:child_process"), async importOriginal => ({ ...await importOriginal(), spawn: vi.fn() }));

let root: string;
let child: ChildProcess;
let tick: number;
let log: MockInstance<typeof console.log>;

beforeEach(() => {
  vi.useFakeTimers();
  root = mkdtempSync(join(tmpdir(), "driichi-producer-control-"));
  tick = 100;
  vi.spyOn(performance, "now").mockImplementation(() => tick);
  // OS process and streams are the boundary; no real compiler or Room is started.
  child = Object.assign(new EventEmitter(), { stdout: new PassThrough(), stderr: new PassThrough(), stdin: new PassThrough(), pid: 12345, kill: vi.fn() }) as unknown as ChildProcess;
  vi.mocked(spawn).mockReturnValue(child);
  vi.spyOn(process, "kill").mockReturnValue(true);
  log = vi.spyOn(console, "log").mockImplementation(() => {});
});
afterEach(() => {
  for (const summary of summaries()) {
    expect(Object.keys(summary).sort()).toEqual(["build", "state", "timeout_ms", "elapsed_ms", "last_output_age_ms", "observed_close", "exit", "stdout_utf8_bytes", "stderr_utf8_bytes", "notices", "last_notice", "stderr_line_overflow_count", "stderr_incomplete_line_count", "counter_saturated"].sort());
    expect(Object.keys(summary.notices).sort()).toEqual(["compiling_dependency", "compiling_core", "compiling_replay", "compiling_server", "compiling_other_workspace", "lock_wait", "finished", "warning", "error", "other"].sort());
    expect(["frontend_build", "cargo_feature_release"]).toContain(summary.build);
    expect(["started", "closed_success", "closed_failure", "process_error", "timeout"]).toContain(summary.state);
    expect(["unknown", ...Object.keys(summary.notices)]).toContain(summary.last_notice);
    for (const count of [...Object.values(summary.notices), summary.stderr_line_overflow_count] as number[]) {
      expect(Number.isInteger(count)).toBe(true); expect(count).toBeGreaterThanOrEqual(0); expect(count).toBeLessThanOrEqual(65535);
    }
  }
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.mocked(spawn).mockReset();
  rmSync(root, { recursive: true, force: true });
});

const build = () => ({ root, target: "private-target", stage: "cargo_feature_release" as const });
const summaries = () => log.mock.calls.filter(call => typeof call[0] === "string" && call[0].startsWith("Task8 producer observation "))
  .map(call => JSON.parse(call[0].slice("Task8 producer observation ".length)));
const receipt = () => JSON.parse(readFileSync(join(root, readdirSync(root).find(name => name.endsWith(".command.json"))!), "utf8"));

it("reports Task8 build start and observed successful close without changing captured output", async () => {
  const pending = runCommand("fake-build", ["private-argument"], "private-directory", undefined, 600_000, build());
  child.stdout!.emit("data", "ready\n");
  child.stderr!.emit("data", "   Compiling double_riichi_server v0.1.0 (private-path)\n");
  tick = 125;
  child.emit("close", 0);
  await expect(pending).resolves.toEqual({ stdout: "ready\n", stderr: "   Compiling double_riichi_server v0.1.0 (private-path)\n" });
  expect(summaries()).toEqual([
    expect.objectContaining({ build: "cargo_feature_release", state: "started", timeout_ms: 600_000, elapsed_ms: 0, last_output_age_ms: null, observed_close: false, exit: null }),
    expect.objectContaining({ build: "cargo_feature_release", state: "closed_success", elapsed_ms: 25, last_output_age_ms: 25, observed_close: true, exit: 0, last_notice: "compiling_server" }),
  ]);
  expect(receipt()).toMatchObject({ observed_close: true, exit: 0, incomplete: false, timed_out: false, timeout_ms: 600_000 });
});

it("keeps split colored notices bounded and unknown text private without inventing progress", async () => {
  const secret = "private-token-and-identity";
  const pending = runCommand("fake-build", [secret], secret, secret, 600_000, build());
  const chunks = ["\u001b[1", "m   Comp", "iling double_riichi_server v0.1.0 (", secret, ")\u001b[0", "m\r\n", "x".repeat(4097), ` Compiling double_riichi_core v0.1.0\n${secret}\n`, "Compiling double_riichi_replay v0.1.0"];
  for (const chunk of chunks) child.stderr!.emit("data", chunk);
  child.emit("close", 0);
  await expect(pending).resolves.toMatchObject({ stderr: chunks.join("") });
  expect(summaries()[1]).toMatchObject({ last_notice: "compiling_server", notices: { compiling_server: 1, compiling_core: 0, compiling_replay: 0, other: 1 }, stderr_line_overflow_count: 1, stderr_incomplete_line_count: 1 });
  expect(JSON.stringify(summaries())).not.toContain(secret);
  expect(readFileSync(join(root, readdirSync(root).find(name => name.endsWith(".stderr.log"))!), "utf8")).toBe(chunks.join(""));
});

it.each([2, null])("preserves a failed observed close code %s instead of inventing a native exit", async code => {
  const pending = runCommand("fake-build", [], "private-directory", undefined, 600_000, build());
  child.stderr!.emit("data", "error: private-error-token\n");
  child.emit("close", code);
  await expect(pending).rejects.toThrow(`failed with exit ${code ?? 1}: error: private-error-token`);
  expect(summaries()).toHaveLength(2);
  expect(summaries()[1]).toMatchObject({ state: "closed_failure", observed_close: true, exit: code, notices: { error: 1 } });
  expect(JSON.stringify(summaries())).not.toContain("private-error-token");
  expect(receipt()).toMatchObject({ observed_close: true, exit: code, incomplete: false, timed_out: false });
  // Original nonzero raw error excerpts are retained; privacy covers new summaries only.
});

it("preserves the original process error without mistaking a subsequent close for a native exit", async () => {
  const error = new Error("private-process-error");
  const pending = runCommand("fake-build", [], "private-directory", undefined, 600_000, build());
  const checked = expect(pending).rejects.toBe(error);
  child.emit("error", error);
  await checked;
  child.emit("close", null);
  expect(summaries()).toHaveLength(2);
  expect(summaries()[1]).toMatchObject({ state: "process_error", observed_close: false, exit: null });
  expect(JSON.stringify(summaries())).not.toContain(error.message);
  expect(receipt()).toMatchObject({ observed_close: false, exit: null, incomplete: true, timed_out: false });
});

it("reports a synchronous spawn-call error without changing its identity or fabricating evidence", async () => {
  const error = new Error("private-invalid-spawn");
  vi.mocked(spawn).mockImplementationOnce(() => { throw error; });
  await expect(runCommand("fake-build", [], "private-directory", undefined, 600_000, build())).rejects.toBe(error);
  expect(summaries().map(summary => summary.state)).toEqual(["started", "process_error"]);
  expect(JSON.stringify(summaries())).not.toContain(error.message);
  expect(readdirSync(root)).toEqual([]);
});

it("keeps the 600s watchdog boundary and freezes unobserved close despite later child events", async () => {
  const pending = runCommand("fake-build", [], "private-directory", undefined, 600_000, build());
  const checked = expect(pending).rejects.toThrow("exceeded 600000ms");
  child.stdout!.emit("data", "private-timeout-output");
  tick = 600099;
  vi.setSystemTime(-10000);
  await vi.advanceTimersByTimeAsync(599999);
  expect(summaries()).toHaveLength(1);
  tick = 600100;
  await vi.advanceTimersByTimeAsync(1);
  await checked;
  expect(summaries()[1]).toMatchObject({ state: "timeout", elapsed_ms: 600000, last_output_age_ms: 600000, observed_close: false, exit: null });
  if (process.platform === "win32") expect(child.kill).toHaveBeenCalledWith("SIGKILL");
  else expect(process.kill).toHaveBeenCalledWith(-12345, "SIGKILL");
  expect(receipt()).toMatchObject({ observed_close: false, exit: null, incomplete: true, timed_out: true, timeout_ms: 600_000 });
  const frozen = JSON.stringify(summaries());
  child.stdout!.emit("data", "late-private-output");
  child.emit("close", 0);
  expect(JSON.stringify(summaries())).toBe(frozen);
  expect(readFileSync(join(root, readdirSync(root).find(name => name.endsWith(".stdout.log"))!), "utf8")).toBe("private-timeout-output");
  expect(frozen).not.toContain("private-timeout-output");
});

it("counts only bounded notice classes and leaves an unrecognized compiler status unknown", async () => {
  const pending = runCommand("fake-build", [], "private-directory", undefined, 600_000, build());
  child.stderr!.emit("data", "private-unknown\n".repeat(65536));
  child.stdout!.emit("data", "☃");
  child.emit("close", 0);
  await pending;
  expect(summaries()[1]).toMatchObject({ last_notice: "unknown", notices: { other: 65535 }, counter_saturated: true, stdout_utf8_bytes: 3 });
  expect(JSON.stringify(summaries())).not.toContain("private-unknown");
});

it("recognizes lock and workspace notices without printing package names or claiming compiler completion", async () => {
  const pending = runCommand("fake-build", [], "private-directory", undefined, 600_000, build());
  child.stderr!.emit("data", "Blocking waiting for file lock on private-cache\nCompiling private-dependency v1.0.0\nCompiling double_riichi_core v0.1.0\nCompiling double_riichi_replay v0.1.0\nCompiling double_riichi_mcp v0.1.0\nwarning: private-warning\nFinished `release` profile [optimized]\n");
  expect(summaries()).toHaveLength(1);
  child.emit("close", 0);
  await pending;
  expect(summaries()[1]).toMatchObject({ last_notice: "finished", notices: { lock_wait: 1, compiling_dependency: 1, compiling_core: 1, compiling_replay: 1, compiling_other_workspace: 1, warning: 1, finished: 1 } });
  expect(JSON.stringify(summaries())).not.toMatch(/private-cache|private-dependency|private-warning/);
});

it("observes the frontend build separately without parsing stdout as Cargo notices", async () => {
  const pending = runCommand("fake-build", [], "private-directory", undefined, 180_000, { ...build(), stage: "frontend_build" });
  child.stdout!.emit("data", "Compiling private-frontend-name v1.0.0\n");
  child.emit("close", 0);
  await pending;
  expect(summaries()).toHaveLength(2);
  expect(summaries()[1]).toMatchObject({ build: "frontend_build", timeout_ms: 180_000, last_notice: "unknown", notices: { compiling_dependency: 0 } });
});

it.each(["hash-password", "taskkill", "default-build"])("adds no telemetry or Task8 capture for inactive %s commands", async command => {
  const pending = runCommand(command, ["private-argument"], "private-directory", "private-stdin");
  child.stdout!.emit("data", "private-nonbuild-output");
  child.emit("close", 0);
  await expect(pending).resolves.toMatchObject({ stdout: "private-nonbuild-output" });
  expect(summaries()).toEqual([]);
  expect(log).not.toHaveBeenCalled();
  expect(readdirSync(root)).toEqual([]);
});
