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

## Optional ChatGPT OAuth gateway

ChatGPT OAuth is opt-in. The gateway exposes `/chatgpt/mcp` only when `[chatgpt_oauth]` is enabled and `DRIICHI_CHATGPT_BOT_TOKEN` is an active Bot Token at startup. If the OAuth configuration is absent or the process secret is missing or inactive, the OAuth discovery and gateway routes stay disabled (the discovery and gateway paths return 404). Existing `/mcp` and MJAI clients continue to use their Bot Tokens.

### Configure a deployment

1. Give the server a stable public hostname with HTTPS. Terminate TLS at a reverse proxy that preserves paths, Authorization and Origin headers, MCP session IDs, and POST/GET/DELETE methods. It must stream MCP responses without buffering. Keep the app bound to a private interface when it sits behind a proxy.
2. Set `public_origin` to the exact public HTTPS origin, with no path. The localhost HTTP value in `config.toml.example` is for local use and cannot enable OAuth. Keep the config data root on durable storage; OAuth grants are stored in its SQLite database.
3. Create one dedicated Bot Token for the ChatGPT gateway through the existing admin API (`POST /api/v1/admin/tokens`). Copy the returned token once into the server service's protected process environment as `DRIICHI_CHATGPT_BOT_TOKEN`. Do not put it in `config.toml`, the config-root `.env`, source control, or a plugin. Do not reuse the Pi or MJAI token. The server checks the token at startup and on each delegated MCP request; revocation or a changed process value stops delegation. The config-root `.env` parser accepts only the admin username and password hash, so do not add the Bot Token there.
4. Copy `config.toml.example` to the server's data root and set `[chatgpt_oauth]` to enabled. Replace `client_id` and `redirect_uri` with the exact metadata URL and callback shown on ChatGPT's connection management page. The values in the example are illustrative and must be verified against that page. Set `allowed_origins` to the trusted ChatGPT HTTPS origin. The OAuth validator requires a publicly reachable HTTPS origin and accepts only ChatGPT metadata, callback, and origin values.
5. Restart the service after changing the configuration or process secret. Check `/.well-known/oauth-protected-resource/chatgpt/mcp` and `/.well-known/oauth-authorization-server` through the public HTTPS origin. The resource identifier must equal the configured HTTPS origin followed by `/chatgpt/mcp`, and its authorization server must match the configured issuer.
6. Before packaging a plugin, test the real endpoint from ChatGPT Developer Mode: metadata discovery, admin sign-in and consent, PKCE token exchange, MCP initialize, join, `get_my_state`, a legal action, and leave. Verify a pre-existing Pi bridge and MJAI client still work. A live ChatGPT plugin package is not part of the release archive; create one only after a stable HTTPS endpoint has been deployed and verified.

From a source checkout, verify the opt-in and startup gates with:

```sh
cargo test -p double_riichi_server --test task6_config_auth_storage chatgpt_oauth_is_opt_in_and_requires_trusted_https_configuration
cargo test -p double_riichi_server --test chatgpt_mcp startup_disables_oauth_routes_without_an_active_dedicated_token
```

`driichi --version` prints the application version and source commit. The
archive's `VERSION`, `RELEASE-METADATA.json`, and `SHA256SUMS` are generated
from the same build. Verify the adjacent `.sha256` file before extraction.

The bundled `character-packs/` directory contains project-generated CC0
Starter Packs only. The release workflow also publishes a versioned standalone
`character-packs-<starter-version>.zip` beside each platform archive. Real
Characters and their trademarks are not included. See `THIRD_PARTY_NOTICES` for
vendored asset attribution and license scope.
