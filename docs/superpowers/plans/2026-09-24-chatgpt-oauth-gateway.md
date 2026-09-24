# ChatGPT OAuth Gateway Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Connect the owner's ChatGPT account to driichi MCP through OAuth while preserving Bot Token authentication for Pi and MJAI.

**Architecture:** Add an optional OAuth gateway inside the Axum server at `/chatgpt/mcp`. After validating an OAuth access token, the gateway replaces the request credential with one dedicated Bot Token and delegates to the existing `McpRuntime`. Add a callable private-state tool, then package a private plugin after the live HTTPS endpoint is verified.

**Tech Stack:** Rust, Axum 0.8.9, rmcp 3.4.0, Tokio, sqlx 0.8.6 / SQLite, SHA-256, HTML consent form, OAuth 2.1 authorization code + PKCE S256, Agent Plugins 1.0.

**Spec:** `docs/superpowers/specs/2026-09-24-chatgpt-oauth-gateway-design.md`

## Global Constraints

- One driichi administrator and one ChatGPT connection; no ordinary user accounts, public distribution, automatic server-initiated turns, or table UI.
- Existing `/mcp` and MJAI Bot Token contracts remain valid, including current session binding, resource projection, revocation, and streaming behavior.
- OAuth must be disabled unless a public HTTPS origin and separate active `DRIICHI_CHATGPT_BOT_TOKEN` are configured.
- Protected resource identifier is the exact configured `https://<public-origin>/chatgpt/mcp`; issuer and resource identifiers remain stable.
- CIMD only; the configured ChatGPT metadata URL and redirect URI must match the verified metadata. No DCR or client secret.
- Authorization code + PKCE S256; token endpoint auth method `none`; authorization and token exchanges require the exact `resource` parameter.
- Access tokens live 10 minutes. Rotating refresh tokens have a maximum family lifetime of 30 days. Scope is exactly `driichi:play`.
- Credentials must never appear in logs, responses, the plugin archive, or URL query strings. Store only hashes and non-secret metadata in SQLite.
- Plugin package name is `driichi-chatgpt`; its `mcp.json` must contain the actual verified HTTPS URL, never a placeholder.

## Review Focus

- Admin Cookie has `Path=/api/v1/admin`: consent must work without broadening that path; test in Task 4.
- Reused refresh tokens and concurrent refresh requests: one winner only, family revoked on replay; test in Task 3.
- Requests with a foreign `Origin` or spoofed internal identity header: reject before Bot Token delegation; test in Task 5.
- A wrong-audience access token that is otherwise valid: reject before MCP initialization; test in Task 5.
- Process restart between consent and refresh: persisted grant remains usable; test in Task 3.

---

## File map and interfaces

| File | Responsibility |
| --- | --- |
| `crates/double_riichi_server/src/mcp.rs` | Add `get_my_state` by reusing the existing private `read_state` projection. |
| `crates/double_riichi_server/src/config.rs`, `config.toml.example` | Optional, strict OAuth gateway configuration and HTTPS validation. |
| `crates/double_riichi_server/migrations/0005_chatgpt_oauth.sql`, `src/storage.rs` | Durable, hashed authorization codes, access tokens, refresh families, and atomic rotation. |
| `crates/double_riichi_server/src/oauth.rs` | OAuth metadata, trusted ChatGPT CIMD validation, admin consent, code/PKCE exchange, token validation, refresh, and revocation. |
| `crates/double_riichi_server/src/oauth_store.rs` | SQL statements and atomic token-family rotation, using the `Storage` pool. |
| `crates/double_riichi_server/src/chatgpt_gateway.rs` | Validate OAuth Bearer access, enforce scope and Origin, substitute the dedicated Bot Token in memory, delegate the streaming request to `McpRuntime`. |
| `crates/double_riichi_server/src/http.rs`, `src/lib.rs`, `src/main.rs` | Wire OAuth state and routes into the existing server and shutdown lifecycle. |
| `crates/double_riichi_server/tests/chatgpt_oauth.rs`, `tests/chatgpt_mcp.rs` | HTTP and MCP integration tests. |
| `plugins/driichi-chatgpt/plugin.json`, `mcp.json`, `skills/interactive-play/SKILL.md` | Portable private plugin source, packaged only with a real deployed URL. |

The existing `ServerState` owns the gateway configuration and an `Arc<OAuthService>` when enabled. `OAuthService::validate_access(&str, &str, &str) -> Result<AccessGrant, OAuthError>` checks the token, resource and scope; `AccessGrant` contains the sole administrator subject and never a Bot Token. `OAuthService::exchange_code(CodeExchange) -> Result<TokenPair, OAuthError>` and `OAuthService::rotate_refresh(RefreshExchange) -> Result<TokenPair, OAuthError>` are the token endpoint entry points. `chatgpt_gateway::mcp_endpoint` receives `Extension<Arc<McpRuntime>>`, `State<Arc<ServerState>>`, and `Request<Body>`, returning `Response`. Give test helpers access to these APIs through the module or integration routes, without exporting secret-bearing types from `lib.rs`.

### Task 1: Callable participant state

**Files:** Modify `crates/double_riichi_server/src/mcp.rs`; test in `crates/double_riichi_server/tests/task14_mcp.rs` and the router's in-module exact-list test.

**Interfaces:** Existing `McpHandler::binding(parts)`, `McpHandler::uri(...)`, and `McpHandler::read_state(entry, uri)` are reused; produces `get_my_state() -> Json<serde_json::Value>` as an MCP tool.

- [ ] **Step 1: Write the failing focused tests.** Extend the exact tool list to include `get_my_state`. In the live MCP test, join one Room, call `get_my_state` and `resources/read` for the returned `state_uri`, and assert equal private projections. Assert an unjoined session gets `session_expired` and another Token cannot read the first participant's hand.

```rust
let state = tool_call(&app, &token, &session_id, 7, "get_my_state", json!({})).await;
let value = tool_value(&state);
let resource = read_resource(&app, &token, &session_id, 8, value["state_uri"].as_str().unwrap()).await;
let resource_state: Value = serde_json::from_str(resource["contents"][0]["text"].as_str().unwrap()).unwrap();
assert_eq!(value["legal_actions"], resource_state["legal_actions"]);
```

- [ ] **Step 2: Run** `cargo test -p double_riichi_server --test task14_mcp live_mcp_discovers_joins_reads_and_keeps_room_binding_permanent` and the exact-list unit test; both must fail for the missing tool.
- [ ] **Step 3: Implement the thin tool using existing binding and projection; do not copy projection logic.**

```rust
#[tool(description = "Read only your bound participant's private Room state and legal actions.")]
async fn get_my_state(
    &self,
    Extension(parts): Extension<axum::http::request::Parts>,
) -> Result<Json<Value>, CallToolResult> {
    let (_, entry) = self.binding(&parts).await.map_err(|e| e.result())?;
    let uri = Self::uri(&entry.room_code, entry.participant_id.as_str(), "state");
    self.read_state(&entry, &uri).await.map(Json).map_err(|e| e.result())
}
```

- [ ] **Step 4: Run** `cargo test -p double_riichi_server --test task14_mcp` and `cargo test -p double_riichi_server --lib`; inspect private-data and exact-list assertions. Commit `feat(mcp): expose bound private state as a tool`.

### Task 2: Opt-in configuration and OAuth discovery

**Files:** Modify `crates/double_riichi_server/src/config.rs`, `src/http.rs`, `src/lib.rs`, `config.toml.example`; create `src/oauth.rs`; test in `tests/chatgpt_oauth.rs` and `tests/task6_config_auth_storage.rs`.

**Interfaces:** Produces `ChatgptOAuthConfig { issuer: Url, resource: Url, client_id: Url, redirect_uri: Url, allowed_origins: Vec<Url> }` from validated runtime configuration. Produces read-only handlers for resource and authorization-server metadata. The configured client metadata URL and callback are exact values obtained from ChatGPT's connection management page; code must reject non-ChatGPT origins and mismatched document fields before granting access. Accept the CIMD document's plural token-endpoint-methods field containing `none`, even when its legacy singular preferred method says `private_key_jwt`.

- [ ] **Step 1: Write failing tests** for disabled-by-default behavior, rejecting `public_origin = "http://..."` when enabled, accepting a trusted HTTPS origin, returning resource metadata for `/chatgpt/mcp`, and advertising `S256`, `none`, and CIMD with an issuer equal to `authorization_servers[0]`.

```rust
assert_eq!(metadata["resource"], "https://driichi.example/chatgpt/mcp");
assert_eq!(metadata["authorization_servers"][0], "https://driichi.example");
assert_eq!(server_metadata["code_challenge_methods_supported"], json!(["S256"]));
assert_eq!(server_metadata["token_endpoint_auth_methods_supported"], json!(["none"]));
```

- [ ] **Step 2: Run** `cargo test -p double_riichi_server --test chatgpt_oauth`; it must fail because the routes do not exist.
- [ ] **Step 3: Add strict config and metadata handlers.** Parse a `[chatgpt_oauth]` table with `enabled`, `client_id`, `redirect_uri`, and `allowed_origins`, reject HTTP origins, credentials embedded in URLs, fragments, and non-HTTPS metadata origins. Compute issuer from `public_origin` and resource from issuer plus `/chatgpt/mcp`. Add `/.well-known/oauth-protected-resource/chatgpt/mcp` and `/.well-known/oauth-authorization-server` routes only when enabled. Use the existing `RuntimeConfig::from_path`, `ServerState::from_config`, and `server_router` seams.

```rust
#[derive(Clone, Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RawChatgptOAuthConfig {
    enabled: bool,
    client_id: String,
    redirect_uri: String,
    allowed_origins: Vec<String>,
}
```

- [ ] **Step 4: Verify trusted CIMD validation.** Introduce an injectable metadata fetcher for tests; permit only the configured ChatGPT HTTPS metadata URL, disable redirects, set a short timeout and response-size cap, and compare the response's `client_id` and exact `redirect_uris` membership before accepting a request. Reject localhost/private IPs, unexpected hosts, and unsupported authentication methods. Cache the validated document briefly and revalidate at authorization time. Run `cargo test -p double_riichi_server --test chatgpt_oauth` and the config tests. Commit `feat(oauth): configure gateway and discovery`.

```rust
fn validate_client_document(config: &ChatgptOAuthConfig, document: &Value, redirect: &Url) -> bool {
    document["client_id"].as_str() == Some(config.client_id.as_str())
        && redirect == &config.redirect_uri
        && document["redirect_uris"].as_array().is_some_and(|uris|
            uris.iter().any(|value| value.as_str() == Some(redirect.as_str())))
}
```

### Task 3: Durable OAuth grants and token exchange

**Files:** Create `crates/double_riichi_server/migrations/0005_chatgpt_oauth.sql`, `src/oauth_store.rs`; modify `src/storage.rs`, `src/oauth.rs`, `crates/double_riichi_server/Cargo.toml`; test in `tests/chatgpt_oauth.rs`.

**Interfaces:** `OAuthService::exchange_code(CodeExchange) -> Result<TokenPair, OAuthError>`, `OAuthService::rotate_refresh(RefreshExchange) -> Result<TokenPair, OAuthError>`, `OAuthService::validate_access(&str, &str, &str) -> Result<AccessGrant, OAuthError>`. `CodeExchange` carries client ID, redirect URI, one-time code, PKCE verifier, resource; `RefreshExchange` carries client ID, refresh token, resource. `TokenPair` carries short-lived access and rotating refresh secrets but never derives `Debug`.

- [ ] **Step 1: Write failing integration tests** for S256 verifier success and mismatch, omitted/wrong resource, wrong client/redirect, single-use code, 10-minute access expiration, 30-day family expiration, restart persistence, and atomic refresh rotation including simultaneous exchanges and reused old refresh token.

```rust
let request = valid_exchange();
let first = service.exchange_code(request.clone()).await.unwrap();
assert!(service.exchange_code(request).await.is_err());
let next = service.rotate_refresh(refresh(&first.refresh_token)).await.unwrap();
assert!(service.rotate_refresh(refresh(&first.refresh_token)).await.is_err());
assert!(service.validate_access(&next.access_token, RESOURCE, "driichi:play").await.is_err());
```

- [ ] **Step 2: Run** `cargo test -p double_riichi_server --test chatgpt_oauth`; confirm the new tests fail for missing grant persistence.
- [ ] **Step 3: Add migration and storage operations.** Define tables for codes, access tokens, refresh families, and refresh tokens. Hash every secret with SHA-256 before insertion; use random 32-byte secrets. Store client, subject=`admin`, redirect, resource, scope, PKCE challenge, issued/expiry timestamps, family ID and consumed/revoked flags. Redeem and rotate under SQLite transactions with conditional updates to guarantee one winner; on reuse revoke the family in the same transaction. Never persist the raw Bot Token.

```sql
CREATE TABLE oauth_codes (
  code_hash BLOB PRIMARY KEY,
  client_id TEXT NOT NULL,
  redirect_uri TEXT NOT NULL,
  resource TEXT NOT NULL,
  scope TEXT NOT NULL,
  subject TEXT NOT NULL CHECK(subject = 'admin'),
  pkce_challenge TEXT NOT NULL,
  expires_at INTEGER NOT NULL,
  consumed_at INTEGER
);
CREATE TABLE oauth_refresh_families (
  family_id TEXT PRIMARY KEY,
  client_id TEXT NOT NULL,
  resource TEXT NOT NULL,
  scope TEXT NOT NULL,
  expires_at INTEGER NOT NULL,
  revoked_at INTEGER
);
CREATE TABLE oauth_refresh_tokens (
  token_hash BLOB PRIMARY KEY,
  family_id TEXT NOT NULL REFERENCES oauth_refresh_families(family_id),
  expires_at INTEGER NOT NULL,
  consumed_at INTEGER
);
CREATE TABLE oauth_access_tokens (
  token_hash BLOB PRIMARY KEY,
  family_id TEXT NOT NULL REFERENCES oauth_refresh_families(family_id),
  resource TEXT NOT NULL,
  scope TEXT NOT NULL,
  expires_at INTEGER NOT NULL
);
```

- [ ] **Step 4: Implement code exchange, validation, and rotation**, checking PKCE as `BASE64URL(SHA256(verifier)) == stored_challenge`, exact client/redirect/resource/scope, token lifetime, and family state. Include no-store cache headers on token responses. Run focused tests plus `cargo test -p double_riichi_server --test task6_config_auth_storage`. Commit `feat(oauth): persist and rotate scoped grants`.

```sql
UPDATE oauth_codes
SET consumed_at = ?
WHERE code_hash = ? AND consumed_at IS NULL AND expires_at > ?
RETURNING client_id, redirect_uri, resource, scope, pkce_challenge;
```

The exchange is successful only when that statement returns exactly one row and the stored challenge matches the verifier; issue the tokens in the same transaction.

### Task 4: Administrator sign-in, consent, and authorization response

**Files:** Modify `src/oauth.rs`, `src/http.rs`; test in `tests/chatgpt_oauth.rs`.

**Interfaces:** `GET /api/v1/admin/oauth/authorize` and same-path POST consent plus `POST /api/v1/admin/oauth/login`, all under the existing admin Cookie path. `POST /oauth/token` uses Task 3's exchange/refresh methods. The authorization endpoint binds code to configured client, redirect, resource, scope, PKCE challenge and admin identity.

- [ ] **Step 1: Write failing HTTP tests** for no session -> sign-in, correct admin credentials -> restricted admin Cookie, denied consent -> redirect error with state and issuer, approved consent -> single-use code + state + issuer, wrong Origin/CSRF token -> 403, and unchanged cookie Path `/api/v1/admin`. Verify no code is minted by GET or by logging in alone.

```rust
assert!(set_cookie.contains("Path=/api/v1/admin"));
assert_eq!(redirect.query_pairs().find(|(k, _)| k == "iss").unwrap().1, issuer);
assert!(!body.contains("DRIICHI_CHATGPT_BOT_TOKEN"));
```

- [ ] **Step 2: Run** `cargo test -p double_riichi_server --test chatgpt_oauth`; tests must fail on missing sign-in and consent routes.
- [ ] **Step 3: Build minimal same-origin HTML forms** at the admin-prefixed URLs. Extract a `pub(crate)` helper in `http.rs` from `admin_login` for password verification, `AdminLoginFailure` throttling, and issuing the existing HttpOnly/Secure/SameSite admin Cookie, then call it from the OAuth sign-in handler; keep the old login endpoint behavior and audit path. Reuse `AdminSessionStore` and strict origin/CSRF validation. Render escaped client name, scope, and requested access; keep raw password and authorization code out of logs. On approval mint a one-use code and redirect only to the exact configured redirect. Return exact `iss` on success and OAuth error redirects; echo the exact `state`.

```rust
fn authorized_redirect(redirect: &Url, code: &str, state: &str, issuer: &Url) -> Url {
    let mut url = redirect.clone();
    url.query_pairs_mut().append_pair("code", code).append_pair("state", state)
        .append_pair("iss", issuer.as_str());
    url
}
```

- [ ] **Step 4: Wire `POST /oauth/token`** for `grant_type=authorization_code` and `grant_type=refresh_token`, `client_id`, and `resource`, with generic `invalid_grant` errors. Use form-encoded OAuth parameters and return `Cache-Control: no-store` with JSON. Run focused tests and `cargo test -p double_riichi_server --test task9_http`. Commit `feat(oauth): add admin consent and token endpoints`.

```rust
match form.grant_type.as_str() {
    "authorization_code" => service.exchange_code(form.into_code_exchange()?).await,
    "refresh_token" => service.rotate_refresh(form.into_refresh_exchange()?).await,
    _ => return oauth_error(StatusCode::BAD_REQUEST, "unsupported_grant_type"),
}
```

### Task 5: Protected MCP delegation and end-to-end compatibility

**Files:** Create `src/chatgpt_gateway.rs`; modify `src/http.rs`, `src/lib.rs`; create `tests/chatgpt_mcp.rs`; adjust `tests/task14_mcp.rs` only for the additive tool.

**Interfaces:** `chatgpt_gateway::mcp_endpoint(Extension(runtime), State(state), request) -> Response`; calls `OAuthService::validate_access(raw, configured_resource, "driichi:play")` then `McpRuntime::handle(state, rewritten_request)`. The raw dedicated Bot Token is only in process memory and is checked active at startup and on every delegated call.

- [ ] **Step 1: Write failing integration tests** for authenticated MCP initialize, `mcp-session-id` reuse, `get_my_state`, legal action, DELETE and streamed GET; reject missing/expired/wrong-audience tokens with 401 + `WWW-Authenticate: Bearer resource_metadata="..."`, insufficient scope with 403, foreign Origin with 403, spoofed identity headers, and OAuth token at legacy `/mcp`. Revoke the dedicated Bot Token and assert the next ChatGPT request fails while an unrelated Pi Token still works.

```rust
let response = app.clone().oneshot(
    Request::post("/chatgpt/mcp")
        .header("Authorization", format!("Bearer {access_token}"))
        .header("Accept", "application/json, text/event-stream")
        .body(Body::from(initialize_json)).unwrap()
).await.unwrap();
assert!(response.status().is_success());
assert!(response.headers().contains_key("mcp-session-id"));
```

- [ ] **Step 2: Run** `cargo test -p double_riichi_server --test chatgpt_mcp`; tests fail because `/chatgpt/mcp` is absent.
- [ ] **Step 3: Implement the boundary.** Remove inbound Authorization and internal identity headers, validate an optional Origin against `allowed_origins`, remove it only after validation, insert `Bearer <dedicated-token>` as the sole Authorization value and call the existing runtime with the unchanged body/stream. Reject conflicting or malformed headers. Preserve response streaming and MCP session semantics. Ensure the legacy `/mcp` handler never accepts OAuth tokens.

```rust
let grant = oauth.validate_access(access, resource, "driichi:play").await?;
assert_eq!(grant.subject, "admin");
request.headers_mut().remove(header::ORIGIN);
request.headers_mut().insert(header::AUTHORIZATION, internal_bot_header);
runtime.handle(state, request).await
```

- [ ] **Step 4: Run** `cargo test -p double_riichi_server --test chatgpt_mcp`, `cargo test -p double_riichi_server --test task14_mcp`, `cargo test -p double_riichi_server --test task13_compat`, `cargo test --workspace`, `cargo check --workspace`, `cargo fmt --all -- --check`, and `git diff --check`. Inspect full output, fix only actual failures, and commit `feat(mcp): bridge OAuth access to dedicated Bot Token`.

### Task 6: Real endpoint, private plugin, and acceptance

**Files:** Modify `config.toml.example`, `release/README.md`; create `plugins/driichi-chatgpt/plugin.json`, `mcp.json`, `skills/interactive-play/SKILL.md` only after the endpoint is known; create the private plugin using Plugin Creator's creation tool.

**Interfaces:** Stable deployed HTTPS `/chatgpt/mcp` URL, verified discovery metadata, ChatGPT management page's CIMD client URL and callback URI, packaged Agent Plugins 1.0 archive with one root directory and no secrets.

- [ ] **Step 1: Document deployment settings** and configure HTTPS, the dedicated process-secret Bot Token, exact client metadata document URL, callback, and allowed Origins. No concrete secret goes in the repository. Include a test showing the gateway is disabled when these values are absent.

```toml
# Gateway stays disabled until the operator supplies an active process secret.
[chatgpt_oauth]
enabled = false
client_id = "https://chatgpt.com/oauth/client.json"
redirect_uri = "https://chatgpt.com/connector_platform_oauth_redirect"
allowed_origins = ["https://chatgpt.com"]
```

- [ ] **Step 2: On an authorized deployment, verify the real URL** with MCP Inspector and ChatGPT Developer Mode: discovery -> admin sign-in and consent -> PKCE token exchange -> join -> `get_my_state` -> action -> leave; check a pre-existing Pi bridge and MJAI client still connect with Bot Tokens. If no stable HTTPS URL or dedicated Bot Token is available, stop here and report this precise missing prerequisite; do not substitute a sample host or publish a broken plugin.
- [ ] **Step 3: Write the plugin manifest and skill with the verified URL.** The manifest sets displayName `driichi`, developer `eitaar`, private workflow prompts, and version `0.1.0`; `mcp.json` has one `streamable-http` server at the deployed URL; `SKILL.md` tells ChatGPT to join, inspect only bound state, submit a legal action when requested, and leave. The skill states that background or automatic turns are not guaranteed. Do not embed admin credentials, Bot Tokens or OAuth tokens.

Construct the MCP manifest from the verified endpoint and validate its scheme and path before packaging:

```python
from urllib.parse import urlparse
from pathlib import Path
from json import dumps
import os
plugin_root = Path("plugins/driichi-chatgpt")
verified_endpoint = os.environ["DRIICHI_VERIFIED_MCP_URL"]
parsed = urlparse(verified_endpoint)
assert parsed.scheme == "https" and parsed.path == "/chatgpt/mcp" and parsed.hostname
mcp_manifest = {
    "$schema": "https://agent-plugins.org/schemas/1.0.0/mcp.schema.json",
    "mcpServers": {"driichi": {"type": "streamable-http", "url": verified_endpoint}},
}
(plugin_root / "mcp.json").write_text(dumps(mcp_manifest, indent=2) + "\n")
```

Write this root manifest with the matching display metadata and actual server URL supplied only through `mcp.json`:

```json
{
  "$schema": "https://agent-plugins.org/schemas/1.0.0/plugin.schema.json",
  "name": "driichi-chatgpt",
  "version": "0.1.0",
  "description": "Play in a driichi Room using authenticated MCP tools.",
  "author": {"name": "eitaar"},
  "extensions": {"com.openai": {"interface": {
    "displayName": "driichi",
    "shortDescription": "Connect to a driichi Room",
    "longDescription": "Join a Room, inspect your own state, and submit legal actions.",
    "developerName": "eitaar",
    "category": "Productivity",
    "capabilities": ["Interactive"],
    "defaultPrompt": ["Join my driichi Room.", "Show my current Room state."]
  }}}
}
```

- [ ] **Step 4: Validate archive contents** (`plugin.json`, `mcp.json`, `skills/interactive-play/SKILL.md`), Agent Plugins 1.0 schema, URLs, no secrets and one root directory. Create the private plugin once via the Plugin Creator tool, record plugin ID and release ID, then test its tool surface and OAuth sign-in from ChatGPT. Commit source files and release documentation on the feature branch; do not merge the draft PR without a separate request.

```python
from shutil import make_archive
from zipfile import ZipFile
archive = make_archive(str(plugin_root), "zip", root_dir=plugin_root.parent, base_dir=plugin_root.name)
with ZipFile(archive) as z:
    names = set(z.namelist())
    assert {f"{plugin_root.name}/plugin.json", f"{plugin_root.name}/mcp.json",
            f"{plugin_root.name}/skills/interactive-play/SKILL.md"} <= names
```

## Final verification

Re-read the spec and mark every requirement against the six tasks. Collect actual output for `cargo test --workspace`, `cargo check --workspace`, `cargo fmt --all -- --check`, and `git diff --check`, as well as the real ChatGPT OAuth/MCP transcript. Report any deployment or account-specific prerequisite with its exact impact. Keep the draft PR reviewable and link the plugin only after Plugin Creator returns a verified plugin ID.
