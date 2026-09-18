# Double Riichi release

This archive is self-contained: the `driichi` server embeds the production
frontend and uses bundled SQLite/rustls behavior. It does not need a system
database, CDN, or a checked-out source tree.

## First run

1. Copy `config.toml.example` to `config.toml`.
2. Copy `.env.example` to `.env`.
3. Run `driichi hash-password` and put the resulting PHC value in
   `ADMIN_PASSWORD_HASH` in `.env`.
4. Set `ADMIN_USERNAME`, then run `driichi --config config.toml`.

The config file's directory is the runtime data root. Runtime configuration,
secrets, databases, and replays are intentionally not included in this
archive. `driichi-mcp` is the optional authenticated MCP bridge; invoke it
with `--server https://host.example/mcp` and `DRIICHI_MCP_TOKEN`.

`driichi --version` prints the application version and source commit. The
archive's `VERSION`, `RELEASE-METADATA.json`, and `SHA256SUMS` are generated
from the same build. Verify the adjacent `.sha256` file before extraction.

The bundled `character-packs/` directory contains project-generated CC0
Starter Packs only. Real Characters and their trademarks are not included.
See `THIRD_PARTY_NOTICES` for vendored asset attribution and license scope.
