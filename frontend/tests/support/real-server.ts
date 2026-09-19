import { createWriteStream, existsSync, mkdirSync, readFileSync, writeFileSync, type WriteStream } from "node:fs";
import { mkdtemp, rm } from "node:fs/promises";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { spawn, type ChildProcess } from "node:child_process";
import { randomBytes } from "node:crypto";

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

function serverBinary(): string {
  return join(repositoryRoot, "target", "debug", process.platform === "win32" ? "driichi.exe" : "driichi");
}

function wait(milliseconds: number): Promise<void> {
  return new Promise((resolvePromise) => setTimeout(resolvePromise, milliseconds));
}

async function runCommand(
  command: string,
  args: string[],
  cwd: string,
  input?: string,
  timeoutMilliseconds = 180_000,
): Promise<{ stdout: string; stderr: string }> {
  const child = spawn(command, args, { cwd, stdio: ["pipe", "pipe", "pipe"] });
  let stdout = "";
  let stderr = "";
  child.stdout.setEncoding("utf8");
  child.stderr.setEncoding("utf8");
  child.stdout.on("data", (chunk: string) => { stdout += chunk; });
  child.stderr.on("data", (chunk: string) => { stderr += chunk; });
  child.stdin.end(input);
  const exit = await new Promise<number>((resolvePromise, reject) => {
    let settled = false;
    const timer = setTimeout(() => {
      if (settled) return;
      settled = true;
      child.kill("SIGKILL");
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
      resolvePromise(code ?? 1);
    });
  });
  if (exit !== 0) {
    throw new Error(`${command} ${args.join(" ")} failed with exit ${exit}: ${stderr.slice(-2_000)}`);
  }
  return { stdout, stderr };
}

async function ensureServerBinary(): Promise<string> {
  if (!existsSync(resolve(frontendRoot, "dist"))) {
    await runCommand(npmCommand(), npmArgs(["run", "build"]), frontendRoot);
  }
  await runCommand(
    cargoCommand(),
    ["build", "--locked", "-p", "double_riichi_server"],
    repositoryRoot,
  );
  const binary = serverBinary();
  if (!existsSync(binary)) throw new Error(`server binary was not produced: ${binary}`);
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
      running.child.kill("SIGTERM");
      if (!(await waitForProcessExit(running)) && running.child.exitCode === null) {
        running.child.kill("SIGKILL");
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

export async function startRealServer(): Promise<RealServerHarness> {
  const binary = await ensureServerBinary();
  const root = await mkdtemp(join(tmpdir(), "driichi-task16-e2e-"));
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
    server = startProcess(binary, ["--config", configPath], repositoryRoot, { ...process.env }, serverLog);
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
        try {
          await rm(root, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 });
        } catch (error) {
          firstError ??= error;
        }
        if (firstError) throw firstError;
      },
    };
  } catch (error) {
    try { await stopProcess(vite); } catch { /* preserve the startup failure */ }
    try { await stopProcess(server); } catch { /* preserve the startup failure */ }
    await rm(root, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 });
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
