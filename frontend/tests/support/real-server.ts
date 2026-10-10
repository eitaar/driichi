import { closeSync, createWriteStream, existsSync, fsyncSync, mkdirSync, openSync, readFileSync, statSync, writeFileSync, type WriteStream } from "node:fs";
import { mkdtemp, rm } from "node:fs/promises";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { execFileSync, spawn, type ChildProcess } from "node:child_process";
import { createHash, randomBytes } from "node:crypto";
import { performance } from "node:perf_hooks";
import { stripVTControlCharacters } from "node:util";

const frontendRoot = process.cwd();
const repositoryRoot = resolve(frontendRoot, "..");
const password = randomBytes(24).toString("base64url");

export interface RealServerHarness {
  readonly frontendOrigin: string;
  readonly rustOrigin: string;
  readonly adminUsername: string;
  readonly adminPassword: string;
  stop(): Promise<void>;
}

export interface RealServerOptions {
  task8?: { fixture: "double-ron" | "double-ron-final"; evidenceRoot: string };
}

interface Task8Build {
  root: string;
  target: string;
}

function preserveUtf8(path: string, text: string): void {
  if (text.includes("\0")) throw new Error(`NUL in evidence: ${path}`);
  const descriptor = openSync(path, "wx");
  try { writeFileSync(descriptor, text, "utf8"); fsyncSync(descriptor); }
  finally { closeSync(descriptor); }
  if (readFileSync(path, "utf8") !== text) throw new Error(`evidence readback mismatch: ${path}`);
}

interface RunningProcess {
  child: ChildProcess;
  log: WriteStream;
  closed: Promise<void>;
}

async function freePort(): Promise<number> {
  const listener = createServer();
  await new Promise<void>((resolvePromise, reject) => {
    listener.once("error", reject);
    listener.listen(0, "127.0.0.1", () => resolvePromise());
  });
  const address = listener.address();
  const port = typeof address === "object" && address ? address.port : 0;
  await new Promise<void>((resolvePromise) => listener.close(() => resolvePromise()));
  if (!port) throw new Error("could not allocate a local test port");
  return port;
}

function npmCommand(): string {
  return process.platform === "win32" ? (process.env.ComSpec ?? "cmd.exe") : "npm";
}

function npmArgs(args: string[]): string[] {
  return process.platform === "win32" ? ["/d", "/s", "/c", "npm.cmd", ...args] : args;
}

function cargoCommand(): string {
  return process.platform === "win32" ? "cargo.exe" : "cargo";
}

function killProcessTree(child: ChildProcess, signal: NodeJS.Signals): void {
  const pid = child.pid;
  if (!pid) return;
  if (process.platform === "win32") {
    child.kill(signal);
    return;
  }
  try {
    process.kill(-pid, signal);
  } catch {
    child.kill(signal);
  }
}

function serverBinary(target?: string): string {
  return join(resolve(repositoryRoot, target ?? process.env.CARGO_TARGET_DIR ?? "target"), "release", process.platform === "win32" ? "driichi.exe" : "driichi");
}

function wait(milliseconds: number): Promise<void> {
  return new Promise((resolvePromise) => setTimeout(resolvePromise, milliseconds));
}

export async function runCommand(
  command: string,
  args: string[],
  cwd: string,
  input?: string,
  timeoutMilliseconds = 180_000,
  task8?: Task8Build & { stage: "frontend_build" | "cargo_feature_release" },
): Promise<{ stdout: string; stderr: string }> {
  const started = Date.now();
  const startedMonotonic = performance.now();
  let stdout = "";
  let stderr = "";
  let timedOut = false;
  let observedCloseCode: number | null | undefined;
  let lastOutput: number | null = null;
  let line = "";
  let discardingLine = false;
  let lineOverflow = 0;
  let counterSaturated = false;
  const notices = { compiling_dependency: 0, compiling_core: 0, compiling_replay: 0, compiling_server: 0, compiling_other_workspace: 0, lock_wait: 0, finished: 0, warning: 0, error: 0, other: 0 };
  let lastNotice: "unknown" | keyof typeof notices = "unknown";
  const increment = (value: number) => { if (value < 65535) return value + 1; counterSaturated = true; return value; };
  const classify = () => {
    const text = stripVTControlCharacters(line).trim();
    if (!text) return;
    const compiling = /^Compiling ([a-zA-Z0-9_-]+) v\S+(?:\s|$)/.exec(text)?.[1];
    const notice = compiling === "double_riichi_core" ? "compiling_core" : compiling === "double_riichi_replay" ? "compiling_replay"
      : compiling === "double_riichi_server" ? "compiling_server" : ["double_riichi_mjai", "double_riichi_mcp"].includes(compiling ?? "") ? "compiling_other_workspace"
      : compiling ? "compiling_dependency" : /^Blocking waiting for file lock(?:\s|$)/.test(text) ? "lock_wait"
      : /^Finished\s/.test(text) ? "finished" : /^warning(?:\[|:)/.test(text) ? "warning" : /^error(?:\[|:)/.test(text) ? "error" : "other";
    notices[notice] = increment(notices[notice]);
    if (notice !== "other") lastNotice = notice;
  };
  const observeStderr = (chunk: string) => {
    for (const character of chunk) {
      if (character === "\n") { if (!discardingLine) classify(); line = ""; discardingLine = false; }
      else if (!discardingLine) {
        if (line.length + character.length <= 4096) line += character;
        else { line = ""; discardingLine = true; lineOverflow = increment(lineOverflow); }
      }
    }
  };
  const report = (state: "started" | "closed_success" | "closed_failure" | "process_error" | "timeout") => {
    if (!task8) return;
    const now = performance.now();
    // Notices describe observed text, never a compiler phase, completion or CPU progress.
    console.log(`Task8 producer observation ${JSON.stringify({ build: task8.stage, state, timeout_ms: timeoutMilliseconds,
      elapsed_ms: now - startedMonotonic, last_output_age_ms: lastOutput === null ? null : now - lastOutput,
      observed_close: observedCloseCode !== undefined, exit: observedCloseCode ?? null,
      stdout_utf8_bytes: Buffer.byteLength(stdout), stderr_utf8_bytes: Buffer.byteLength(stderr),
      notices, last_notice: lastNotice, stderr_line_overflow_count: lineOverflow,
      stderr_incomplete_line_count: line || discardingLine ? 1 : 0, counter_saturated: counterSaturated })}`);
  };
  report("started");
  let child;
  try {
    child = spawn(command, args, {
      cwd,
      env: task8 ? { ...process.env, CARGO_TARGET_DIR: task8.target } : process.env,
      stdio: ["pipe", "pipe", "pipe"],
      detached: process.platform !== "win32",
    });
  } catch (error) { report("process_error"); throw error; }
  child.stdout.setEncoding("utf8");
  child.stderr.setEncoding("utf8");
  child.stdout.on("data", (chunk: string) => { stdout += chunk; if (task8) lastOutput = performance.now(); });
  child.stderr.on("data", (chunk: string) => { stderr += chunk; if (task8) { lastOutput = performance.now(); observeStderr(chunk); } });
  child.stdin.end(input);
  let exit: number;
  try {
    exit = await new Promise<number>((resolvePromise, reject) => {
    let settled = false;
    const timer = setTimeout(() => {
      if (settled) return;
      settled = true;
      timedOut = true;
      killProcessTree(child, "SIGKILL");
      reject(new Error(`${command} ${args.join(" ")} exceeded ${timeoutMilliseconds}ms`));
    }, timeoutMilliseconds);
    child.once("error", (error) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      reject(error);
    });
    child.once("close", (code) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      observedCloseCode = code;
      resolvePromise(code ?? 1);
    });
    });
  } catch (error) {
    report(timedOut ? "timeout" : "process_error");
    if (task8) {
      const name = `${started}-${randomBytes(4).toString("hex")}`;
      preserveUtf8(join(task8.root, `${name}.stdout.log`), stdout);
      preserveUtf8(join(task8.root, `${name}.stderr.log`), stderr);
      preserveUtf8(join(task8.root, `${name}.command.json`), JSON.stringify({ command, args, cwd, target: task8.target, pid: child.pid, started, ended: Date.now(), exit: null, observed_close: false, incomplete: true, timed_out: timedOut, timeout_ms: timeoutMilliseconds, error: error instanceof Error ? { name: error.name, message: error.message, stack: error.stack } : String(error) }));
    }
    throw error;
  }
  report(observedCloseCode === 0 ? "closed_success" : "closed_failure");
  if (task8) {
    const name = `${started}-${randomBytes(4).toString("hex")}`;
    preserveUtf8(join(task8.root, `${name}.command.json`), JSON.stringify({ command, args, cwd, target: task8.target, pid: child.pid, started, ended: Date.now(), exit: observedCloseCode, observed_close: true, incomplete: false, timed_out: false, timeout_ms: timeoutMilliseconds }));
    preserveUtf8(join(task8.root, `${name}.stdout.log`), stdout);
    preserveUtf8(join(task8.root, `${name}.stderr.log`), stderr);
  }
  if (exit !== 0) {
    throw new Error(`${command} ${args.join(" ")} failed with exit ${exit}: ${stderr.slice(0, 2_000)}\n${stderr.slice(-2_000)}\nstdout: ${stdout.slice(-2_000)}`);
  }
  return { stdout, stderr };
}

async function ensureServerBinary(task8?: Task8Build): Promise<string> {
  const proofPath = process.env.DRIICHI_TASK8_VERIFIED_BUILD;
  if (task8 && proofPath) {
    const proof = JSON.parse(readFileSync(proofPath, "utf8")) as { sourceBefore: string; sourceAfter: string; buildCommand: string; binaryReceipt: string };
    const before = JSON.parse(readFileSync(proof.sourceBefore, "utf8")) as Record<string, { sha256: string }>;
    const after = JSON.parse(readFileSync(proof.sourceAfter, "utf8"));
    const names = [...new Set(execFileSync("git", ["ls-files", "-co", "--exclude-standard", "-z"], { cwd: repositoryRoot, encoding: "utf8" }).split("\0"))]
      .filter(name => name && !["node_modules/", ".vitest/", "scripts/release/__pycache__/", "frontend/playwright-real-report/", "frontend/playwright-report/"].some(prefix => name.startsWith(prefix))
        && existsSync(resolve(repositoryRoot, name)) && statSync(resolve(repositoryRoot, name)).isFile()).sort();
    if (JSON.stringify(names) !== JSON.stringify(Object.keys(before).sort()) || JSON.stringify(before) !== JSON.stringify(after)) throw new Error("verified build source coverage/custody mismatch");
    for (const name of names) {
      // Only test orchestration may differ; every compiled/product input must remain exact.
      if (name === "frontend/tests/kyoku-results-real-server.spec.ts" || name === "frontend/tests/support/real-server.ts") continue;
      if (createHash("sha256").update(readFileSync(resolve(repositoryRoot, name))).digest("hex") !== before[name].sha256) throw new Error(`verified build source mismatch: ${name}`);
    }
    const build = JSON.parse(readFileSync(proof.buildCommand, "utf8"));
    const args = ["build", "--release", "--locked", "-p", "double_riichi_server", "--features", "task8-test-support", "-j", "1"];
    if (build.exit !== 0 || build.observed_close !== true || build.incomplete !== false || build.timed_out !== false
      || resolve(build.cwd) !== repositoryRoot || JSON.stringify(build.args) !== JSON.stringify(args)) throw new Error("verified build command mismatch");
    const recorded = JSON.parse(readFileSync(proof.binaryReceipt, "utf8"));
    const binary = serverBinary(task8.target);
    const sha256 = createHash("sha256").update(readFileSync(binary)).digest("hex");
    if (resolve(recorded.binary) !== binary || resolve(recorded.target) !== task8.target || recorded.sha256 !== sha256
      || JSON.stringify(recorded.features) !== JSON.stringify(["task8-test-support"])) throw new Error("verified build binary/feature mismatch");
    preserveUtf8(join(task8.root, "binary.json"), JSON.stringify({ binary, sha256, features: ["task8-test-support"], target: task8.target }));
    preserveUtf8(join(task8.root, "reused-build-proof.json"), JSON.stringify({ proofPath, ...proof }));
    return binary;
  }
  // Never embed an older dist left by a previous run or restored by the release gate.
  await runCommand(npmCommand(), npmArgs(["run", "build"]), frontendRoot, undefined, 180_000, task8 && { ...task8, stage: "frontend_build" });
  await runCommand(
    cargoCommand(),
    ["build", "--release", "--locked", "-p", "double_riichi_server", ...(task8 ? ["--features", "task8-test-support", "-j", "1"] : [])],
    repositoryRoot,
    undefined,
    600_000, // Cold release builds exceed the ordinary command timeout.
    task8 && { ...task8, stage: "cargo_feature_release" },
  );
  const binary = serverBinary(task8?.target);
  if (!existsSync(binary)) throw new Error(`server binary was not produced: ${binary}`);
  if (task8) preserveUtf8(join(task8.root, "binary.json"), JSON.stringify({ binary, sha256: createHash("sha256").update(readFileSync(binary)).digest("hex"), features: ["task8-test-support"], target: task8.target }));
  return binary;
}

function writePack(
  root: string,
  id: string,
  usage: "human" | "mjai" | "builtin" | "mcp",
  name: string,
  icon: Buffer,
  portrait: Buffer,
): void {
  const pack = join(root, id);
  mkdirSync(join(pack, "voices"), { recursive: true });
  writeFileSync(join(pack, "manifest.json"), JSON.stringify({ id, name, usage }));
  writeFileSync(join(pack, "LICENSE"), "CC0 1.0 Universal\n");
  writeFileSync(join(pack, "icon.webp"), icon);
  writeFileSync(join(pack, "portrait.webp"), portrait);
  for (const voice of ["chi", "pon", "kan", "riichi", "ron", "tsumo"]) {
    writeFileSync(join(pack, "voices", `${voice}.ogg`), Buffer.from("OggS\\0task16"));
  }
}

async function waitForHttp(url: string, timeoutMilliseconds = 60_000): Promise<void> {
  const deadline = Date.now() + timeoutMilliseconds;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(url);
      if (response.status >= 200 && response.status < 500) return;
    } catch {
      // The listener may need a few turns to finish binding and loading storage.
    }
    await wait(100);
  }
  throw new Error(`local test server did not become ready: ${url}`);
}

const PROCESS_STOP_TIMEOUT_MS = 8_000;

async function waitForProcessExit(running: RunningProcess): Promise<boolean> {
  if (running.child.exitCode !== null) {
    await running.closed;
    return true;
  }
  return Promise.race([
    running.closed.then(() => true),
    wait(PROCESS_STOP_TIMEOUT_MS).then(() => false),
  ]);
}

async function stopProcess(running: RunningProcess | undefined): Promise<void> {
  if (!running) return;
  if (running.child.exitCode === null) {
    const pid = running.child.pid;
    if (pid && process.platform === "win32") {
      try {
        await runCommand(
          "taskkill.exe",
          ["/pid", String(pid), "/t", "/f"],
          repositoryRoot,
          undefined,
          PROCESS_STOP_TIMEOUT_MS,
        );
      } catch {
        if (running.child.exitCode === null) running.child.kill("SIGKILL");
      }
    } else if (pid) {
      killProcessTree(running.child, "SIGTERM");
      if (!(await waitForProcessExit(running)) && running.child.exitCode === null) {
        killProcessTree(running.child, "SIGKILL");
      }
    }
  }
  if (!(await waitForProcessExit(running))) {
    throw new Error(`process ${running.child.pid ?? "unknown"} did not exit within ${PROCESS_STOP_TIMEOUT_MS}ms`);
  }
}

function startProcess(
  command: string,
  args: string[],
  cwd: string,
  environment: NodeJS.ProcessEnv,
  logPath: string,
): RunningProcess {
  const child = spawn(command, args, {
    cwd,
    env: environment,
    stdio: ["ignore", "pipe", "pipe"],
    detached: process.platform !== "win32",
  });
  const log = createWriteStream(logPath, { flags: "a" });
  child.stdout?.pipe(log, { end: false });
  child.stderr?.pipe(log, { end: false });
  child.on("error", () => {
    // Readiness polling reports the bounded startup failure to the test.
  });
  const closed = new Promise<void>((resolvePromise) => {
    child.once("close", () => {
      child.stdout?.unpipe(log);
      child.stderr?.unpipe(log);
      if (log.writableEnded) resolvePromise();
      else log.end(() => resolvePromise());
    });
  });
  return { child, log, closed };
}

export async function startRealServer(options: RealServerOptions = {}): Promise<RealServerHarness> {
  if (options.task8 && !["double-ron", "double-ron-final"].includes(options.task8.fixture)) throw new Error("unknown Task8 fixture");
  if (options.task8) mkdirSync(options.task8.evidenceRoot, { recursive: true });
  const task8Root = options.task8 ? await mkdtemp(join(options.task8.evidenceRoot, "task8-")) : undefined;
  const task8 = options.task8 && task8Root ? { root: task8Root, target: resolve(repositoryRoot, "target", "task8-be72f5cf") } : undefined;
  const binary = await ensureServerBinary(task8);
  const root = task8Root ?? await mkdtemp(join(tmpdir(), "driichi-task16-e2e-"));
  let server: RunningProcess | undefined;
  let vite: RunningProcess | undefined;
  try {
    const characterRoot = join(root, "character-packs");
    mkdirSync(characterRoot, { recursive: true });
    const fixtureRoot = resolve(frontendRoot, "tests", "fixtures", "task12-characters");
    const humanIcon = readFileSync(join(fixtureRoot, "player-red", "icon.webp"));
    const humanPortrait = readFileSync(join(fixtureRoot, "player-red", "portrait.webp"));
    const botIcon = readFileSync(join(fixtureRoot, "tsumogiri-bot", "icon.webp"));
    const botPortrait = readFileSync(join(fixtureRoot, "tsumogiri-bot", "portrait.webp"));
    writePack(characterRoot, "player-red", "human", "Player Red", humanIcon, humanPortrait);
    writePack(characterRoot, "mjai-bot", "mjai", "MJAI Bot", botIcon, botPortrait);
    writePack(characterRoot, "tsumogiri-bot", "builtin", "Tsumogiri Bot", botIcon, botPortrait);
    writePack(characterRoot, "mcp-agent", "mcp", "MCP Agent", botIcon, botPortrait);

    const rustPort = await freePort();
    const frontendPort = await freePort();
    const rustOrigin = `http://127.0.0.1:${rustPort}`;
    const frontendOrigin = `http://127.0.0.1:${frontendPort}`;
    const configPath = join(root, "config.toml");
    const envPath = join(root, ".env");
    writeFileSync(
      configPath,
      [
        `bind = "127.0.0.1:${rustPort}"`,
        `public_origin = "${frontendOrigin}"`,
        "tracing_format = \"text\"",
        "unlimited_watchdog_seconds = 60",
        "empty_room_cleanup_seconds = 60",
        "shutdown_seconds = 3",
        "",
        "[characters]",
        'mjai = "mjai-bot"',
        'builtin = "tsumogiri-bot"',
        'mcp = "mcp-agent"',
        "",
        "[time_controls.casual]",
        "turn_seconds = 1",
        "response_seconds = 1",
        "",
      ].join("\n"),
    );
    const hashOutput = await runCommand(
      binary,
      ["hash-password"],
      repositoryRoot,
      `${password}\n${password}\n`,
    );
    const passwordHash = hashOutput.stdout.trim().split(/\r?\n/).at(-1);
    if (!passwordHash?.startsWith("$argon2")) throw new Error("server password hash was not generated");
    writeFileSync(envPath, `ADMIN_USERNAME=admin\nADMIN_PASSWORD_HASH=${passwordHash}\n`);

    const serverLog = join(root, "server.log");
    const viteLog = join(root, "vite.log");
    server = startProcess(binary, ["--config", configPath], repositoryRoot, { ...process.env, ...(options.task8 ? { DRIICHI_TASK8_FIXTURE: options.task8.fixture } : {}) }, serverLog);
    await waitForHttp(`${rustOrigin}/status`);
    vite = startProcess(
      npmCommand(),
      npmArgs(["run", "dev", "--", "--host", "127.0.0.1", "--port", String(frontendPort)]),
      frontendRoot,
      { ...process.env, DRIICHI_E2E_SERVER_ORIGIN: rustOrigin },
      viteLog,
    );
    await waitForHttp(frontendOrigin);

    let stopped = false;
    return {
      frontendOrigin,
      rustOrigin,
      adminUsername: "admin",
      adminPassword: password,
      async stop() {
        if (stopped) return;
        stopped = true;
        let firstError: unknown;
        for (const process of [vite, server]) {
          try { await stopProcess(process); }
          catch (error) { firstError ??= error; }
        }
        if (!task8) {
          try {
            await rm(root, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 });
          } catch (error) {
            firstError ??= error;
          }
        }
        if (firstError) throw firstError;
      },
    };
  } catch (error) {
    try { await stopProcess(vite); } catch { /* preserve the startup failure */ }
    try { await stopProcess(server); } catch { /* preserve the startup failure */ }
    if (!task8) await rm(root, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 });
    throw error;
  }
}

export function ensureScreenshotDirectory(): string {
  const directory = resolve(frontendRoot, "test-results", "task-16-review");
  mkdirSync(directory, { recursive: true });
  return directory;
}

export function screenshotPath(directory: string, name: string): string {
  return join(directory, `${name}.png`);
}
