# Driichi

Driichi is a mahjong match environment built to test and evaluate AIs that use the **mjai protocol**. Humans, mjai bots, MCP-connected LLM agents, and built-in bots can take seats at the same table — including matches such as **human vs. LLM** or **LLM vs. mjai bot**.

## What it does

- **Mixed-player matches:** Play in a browser alongside mjai bots and MCP-controlled agents.
- **AI evaluation:** Run bot-only benchmarks with fixed rosters, seat rotation, recorded results, and replays.
- **Riichi mahjong:** Host three- or four-player Room Matches, with a rules engine and server-authoritative legal actions.
- **Agent connectivity:** Connect mjai-compatible clients and LLM agents through the MCP interface.

> **Compatibility note:** Three-player MJAI is not supported in the current v1 scope. The riichi.dev-compatible MJAI adapter is still undergoing external compatibility validation.

## Getting started

There are no published GitHub Release artifacts yet, so build a local release package from source.

Prerequisites: Git, **Rust 1.95+**, Node.js/npm, Python 3, and FFmpeg. Supported release targets are Linux (x86_64/ARM64) and Windows (x86_64).

```sh
git clone https://github.com/eitaar/driichi.git
cd driichi
python scripts/release/package.py build --platform auto
```

The generated archive is placed in `target/release-artifacts/`. Extract it, then:

1. Copy `config.toml.example` to `config.toml` and `.env.example` to `.env`.
2. Run `driichi hash-password` (or `./driichi hash-password` on Unix) and set the resulting hash as `ADMIN_PASSWORD_HASH` in `.env`.
3. Set `ADMIN_USERNAME` in `.env`, then start the server with `driichi --config config.toml` (or `./driichi --config config.toml`).
4. Open `http://127.0.0.1:3000` in your browser.

The configuration directory stores runtime data and credentials. Do not commit `.env`, Bot Tokens, or other secrets.

For additional deployment details, see [the release guide](release/README.md).

## Connecting agents

**mjai bots:** Driichi provides an mjai adapter, including a riichi.dev-compatible endpoint, for external mahjong engines. See the [implementation specification](spec/implementation-v1.md) for the current protocol scope.

**LLM agents (MCP):** Authenticate an agent with a Bot Token and connect it to the server's `/mcp` endpoint. The optional `driichi-mcp` binary exposes that endpoint to stdio-based MCP clients:

```sh
DRIICHI_MCP_TOKEN=<your-bot-token> driichi-mcp --server http://127.0.0.1:3000/mcp
```

The direct ChatGPT OAuth gateway is optional and requires a separately configured, verified HTTPS deployment; it is not enabled by default.

**Humans:** Join a Room through the browser. The Room's selected participants can include human players, mjai bots, MCP agents, or built-in bots.

## Benchmarks and replays

Use **Benchmark Mode** for bot-only evaluation. A Benchmark Run plays a fixed roster across multiple recorded matches while rotating seats, so results can be compared by participant. Human players cannot take seats in Benchmark Mode; use ordinary Rooms for human-vs-AI matches.

Completed matches can be recorded and inspected through the replay system.

## Development

Run the repository's local checks with:

```sh
just check
```

The codebase is organized around a Rust match engine/server, mjai and MCP adapters, a replay component, and a React/TypeScript frontend. See [the implementation specification](spec/implementation-v1.md) and [project terminology](CONTEXT.md) for design details.
