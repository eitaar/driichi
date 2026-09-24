use std::{str::FromStr, sync::Arc, time::Duration};

use axum::{
    Extension,
    body::{Body, to_bytes},
    extract::State,
    http::{HeaderMap, HeaderValue, Method, Request, StatusCode, header},
    response::Response,
};
use serde_json::{Value, json};
use zeroize::Zeroizing;

use crate::{ServerState, mcp::{MCP_MAX_BODY_BYTES, McpRuntime}, oauth::OAuthError};

const CHATGPT_TOOL_SCOPE: &str = "driichi:play";
const CHATGPT_RESPONSE_BODY_TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) struct DedicatedBotToken {
    raw: Arc<Zeroizing<String>>,
    token_id: String,
    check_environment: bool,
}

impl DedicatedBotToken {
    pub(crate) fn for_process(raw: String, token_id: String) -> Self {
        Self {
            raw: Arc::new(Zeroizing::new(raw)),
            token_id,
            check_environment: true,
        }
    }

    pub(crate) fn for_tests(raw: String, token_id: String) -> Self {
        Self {
            raw: Arc::new(Zeroizing::new(raw)),
            token_id,
            check_environment: false,
        }
    }

    fn is_active(&self, state: &ServerState) -> bool {
        if self.check_environment
            && std::env::var("DRIICHI_CHATGPT_BOT_TOKEN").ok().as_deref() != Some(self.raw.as_str())
        {
            return false;
        }
        state
            .bot_token_record_for_chatgpt(self.raw.as_str())
            .is_some_and(|record| record.token_id() == self.token_id.as_str())
    }
}

pub(crate) async fn mcp_endpoint(
    Extension(runtime): Extension<Arc<McpRuntime>>,
    State(state): State<Arc<ServerState>>,
    mut request: Request<Body>,
) -> Response {
    if request.headers().keys().any(is_spoofed_identity_header) {
        return gateway_error(StatusCode::FORBIDDEN, "spoofed_identity_header", None);
    }

    let Some(oauth) = state.chatgpt_oauth.as_ref() else {
        return gateway_error(StatusCode::SERVICE_UNAVAILABLE, "oauth_unavailable", None);
    };
    if !origin_allowed(request.headers(), &oauth.config.allowed_origins) {
        return gateway_error(StatusCode::FORBIDDEN, "origin_not_allowed", None);
    }

    let Some(token) = state.chatgpt_bot_token.as_ref() else {
        return gateway_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "delegation_unavailable",
            None,
        );
    };
    if !token.is_active(&state) {
        return gateway_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "delegation_unavailable",
            None,
        );
    }

    // Inspect only bounded POST bodies, then replay the exact bytes to rmcp.
    // GET-based SSE streams and all other non-POST requests remain untouched.
    let message = match request_json_message(&mut request).await {
        Ok(message) => message,
        Err(response) => return response,
    };
    let rpc_method = message.as_ref().and_then(json_rpc_method).map(str::to_owned);
    let rpc_id = message.as_ref().and_then(json_rpc_id);
    let is_tool_call = rpc_method.as_deref() == Some("tools/call");
    let is_tools_list = rpc_method.as_deref() == Some("tools/list");
    let anonymous_discovery = rpc_method
        .as_deref()
        .is_some_and(|method| anonymous_discovery_method(method, rpc_id.as_ref()));
    let authorization_supplied = request
        .headers()
        .get_all(header::AUTHORIZATION)
        .iter()
        .next()
        .is_some();

    match bearer_token(request.headers()) {
        Some(access_token) => {
            let grant = match oauth
                .grants
                .validate_access(access_token, oauth.config.resource.as_str(), CHATGPT_TOOL_SCOPE)
                .await
            {
                Ok(grant) => grant,
                Err(OAuthError::InsufficientScope) if is_tool_call => {
                    return tool_auth_error_response(
                        &oauth.config.resource,
                        rpc_id.as_ref(),
                        single_session_header(request.headers()),
                        ToolAuthFailure::InsufficientScope,
                    );
                }
                Err(OAuthError::InsufficientScope) => {
                    return gateway_error(
                        StatusCode::FORBIDDEN,
                        "insufficient_scope",
                        Some(scope_challenge(&oauth.config.resource)),
                    );
                }
                Err(OAuthError::InvalidGrant) if is_tool_call => {
                    return tool_auth_error_response(
                        &oauth.config.resource,
                        rpc_id.as_ref(),
                        single_session_header(request.headers()),
                        ToolAuthFailure::Invalid,
                    );
                }
                Err(OAuthError::InvalidGrant) => {
                    return unauthorized(&oauth.config.resource, "invalid_token");
                }
                Err(OAuthError::Storage) => {
                    return gateway_error(StatusCode::SERVICE_UNAVAILABLE, "oauth_unavailable", None);
                }
            };
            if grant.subject != "admin"
                || grant.resource.as_str() != oauth.config.resource.as_str()
                || grant.client_id.as_str() != oauth.config.client_id.as_str()
            {
                return if is_tool_call {
                    tool_auth_error_response(
                        &oauth.config.resource,
                        rpc_id.as_ref(),
                        single_session_header(request.headers()),
                        ToolAuthFailure::Invalid,
                    )
                } else {
                    unauthorized(&oauth.config.resource, "invalid_token")
                };
            }
        }
        None if !authorization_supplied && anonymous_discovery => {}
        None if is_tool_call => {
            let failure = if authorization_supplied {
                ToolAuthFailure::Invalid
            } else {
                ToolAuthFailure::Missing
            };
            return tool_auth_error_response(
                &oauth.config.resource,
                rpc_id.as_ref(),
                single_session_header(request.headers()),
                failure,
            );
        }
        None => return unauthorized(&oauth.config.resource, "invalid_token"),
    }

    let authorization = format!("Bearer {}", token.raw.as_str());
    let internal_authorization = match HeaderValue::from_str(&authorization) {
        Ok(value) => value,
        Err(_) => {
            return gateway_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "delegation_unavailable",
                None,
            );
        }
    };

    request.headers_mut().remove(header::AUTHORIZATION);
    request.headers_mut().remove(header::ORIGIN);
    request
        .headers_mut()
        .insert(header::AUTHORIZATION, internal_authorization);
    let response = runtime.handle(state, request).await;

    if is_tools_list && let Some(id) = rpc_id.as_ref() {
        return decorate_tools_list_response(response, id).await;
    }
    response
}

async fn request_json_message(request: &mut Request<Body>) -> Result<Option<Value>, Response> {
    if request.method() != Method::POST {
        return Ok(None);
    }

    let body = std::mem::replace(request.body_mut(), Body::empty());
    let bytes = match to_bytes(body, MCP_MAX_BODY_BYTES).await {
        Ok(bytes) => bytes,
        Err(_) => {
            return Err(gateway_error(
                StatusCode::PAYLOAD_TOO_LARGE,
                "request_too_large",
                None,
            ));
        }
    };
    let message = serde_json::from_slice::<Value>(&bytes).ok();
    *request.body_mut() = Body::from(bytes);
    Ok(message)
}

fn json_rpc_method(message: &Value) -> Option<&str> {
    (message.get("jsonrpc")?.as_str()? == "2.0")
        .then(|| message.get("method")?.as_str())
        .flatten()
}

fn json_rpc_id(message: &Value) -> Option<Value> {
    let id = message.get("id")?;
    matches!(id, Value::Null | Value::String(_) | Value::Number(_)).then(|| id.clone())
}

fn anonymous_discovery_method(method: &str, id: Option<&Value>) -> bool {
    match method {
        "initialize" | "tools/list" => id.is_some(),
        "notifications/initialized" => id.is_none(),
        _ => false,
    }
}

#[derive(Clone, Copy)]
enum ToolAuthFailure {
    Missing,
    Invalid,
    InsufficientScope,
}

fn single_session_header(headers: &HeaderMap) -> Option<HeaderValue> {
    let mut values = headers.get_all("mcp-session-id").iter();
    let value = values.next()?.clone();
    values.next().is_none().then_some(value)
}

fn tool_auth_error_response(
    resource: &url::Url,
    request_id: Option<&Value>,
    session_id: Option<HeaderValue>,
    failure: ToolAuthFailure,
) -> Response {
    let Some(request_id) = request_id else {
        return unauthorized(resource, "invalid_token");
    };
    let (error, description, include_scope) = match failure {
        ToolAuthFailure::Missing => (
            "insufficient_scope",
            "No OAuth access token was provided. Connect your account to continue.",
            true,
        ),
        ToolAuthFailure::Invalid => (
            "invalid_token",
            "The OAuth access token is invalid or expired. Reconnect your account to continue.",
            false,
        ),
        ToolAuthFailure::InsufficientScope => (
            "insufficient_scope",
            "The OAuth access token does not include the required scope.",
            true,
        ),
    };
    let mut challenge = tool_challenge_value(resource, error, description);
    if include_scope {
        challenge.push_str(", scope=\"");
        challenge.push_str(CHATGPT_TOOL_SCOPE);
        challenge.push('"');
    }
    let body = json!({
        "jsonrpc": "2.0",
        "id": request_id,
        "result": {
            "content": [{
                "type": "text",
                "text": "Authentication required to use this tool. Connect your account to continue."
            }],
            "_meta": {"mcp/www_authenticate": [challenge]},
            "isError": true
        }
    });
    let mut response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::from(body.to_string()))
        .expect("tool authentication response is valid");
    if let Some(session_id) = session_id {
        response.headers_mut().insert("mcp-session-id", session_id);
    }
    response
}

fn tool_challenge_value(resource: &url::Url, error: &str, description: &str) -> String {
    let mut metadata = resource.clone();
    metadata.set_path("/.well-known/oauth-protected-resource/chatgpt/mcp");
    metadata.set_query(None);
    metadata.set_fragment(None);
    format!(
        "Bearer resource_metadata=\"{}\", error=\"{}\", error_description=\"{}\"",
        metadata.as_str(),
        error,
        description,
    )
}

async fn decorate_tools_list_response(response: Response, request_id: &Value) -> Response {
    let (mut parts, body) = response.into_parts();
    if !parts.status.is_success() {
        return Response::from_parts(parts, Body::new(body));
    }

    let content_type = parts
        .headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    let media_type = content_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim();
    let is_json = media_type.eq_ignore_ascii_case("application/json");
    let is_sse = media_type.eq_ignore_ascii_case("text/event-stream");
    if !is_json && !is_sse {
        return invalid_tools_list_response(parts);
    }

    let bytes = match tokio::time::timeout(
        CHATGPT_RESPONSE_BODY_TIMEOUT,
        to_bytes(body, MCP_MAX_BODY_BYTES),
    )
    .await
    {
        Ok(Ok(bytes)) => bytes,
        _ => return invalid_tools_list_response(parts),
    };
    let decorated = if is_json {
        decorate_tools_list_json(&bytes, request_id)
    } else {
        decorate_tools_list_sse(&bytes, request_id)
    };
    let Some(body) = decorated else {
        return invalid_tools_list_response(parts);
    };
    parts.headers.remove(header::CONTENT_LENGTH);
    Response::from_parts(parts, Body::from(body))
}

fn decorate_tools_list_json(bytes: &[u8], request_id: &Value) -> Option<Vec<u8>> {
    let mut message = serde_json::from_slice::<Value>(bytes).ok()?;
    if message.get("id") != Some(request_id) {
        return None;
    }
    if message.get("error").is_some() {
        return Some(bytes.to_vec());
    }
    decorate_tools_list_message(&mut message, request_id)?;
    serde_json::to_vec(&message).ok()
}

fn decorate_tools_list_sse(bytes: &[u8], request_id: &Value) -> Option<Vec<u8>> {
    let text = std::str::from_utf8(bytes).ok()?.replace("\r\n", "\n");
    let mut output = String::new();
    let mut found_response = false;

    for frame in text.split("\n\n").filter(|frame| !frame.is_empty()) {
        let data = frame
            .lines()
            .filter_map(|line| {
                let value = line.strip_prefix("data:")?;
                Some(value.strip_prefix(' ').unwrap_or(value))
            })
            .collect::<Vec<_>>();
        if data.is_empty() {
            output.push_str(frame);
            output.push_str("\n\n");
            continue;
        }

        let payload = data.join("\n");
        let Ok(mut message) = serde_json::from_str::<Value>(&payload) else {
            output.push_str(frame);
            output.push_str("\n\n");
            continue;
        };
        if message.get("id") != Some(request_id) {
            output.push_str(frame);
            output.push_str("\n\n");
            continue;
        }
        if found_response {
            return None;
        }
        found_response = true;
        if message.get("error").is_some() {
            output.push_str(frame);
            output.push_str("\n\n");
            continue;
        }
        decorate_tools_list_message(&mut message, request_id)?;
        let payload = serde_json::to_string(&message).ok()?;
        let mut replaced_data = false;
        for line in frame.lines() {
            if line.starts_with("data:") {
                if !replaced_data {
                    output.push_str("data: ");
                    output.push_str(&payload);
                    output.push('\n');
                    replaced_data = true;
                }
            } else {
                output.push_str(line);
                output.push('\n');
            }
        }
        if !replaced_data {
            return None;
        }
        output.push('\n');
    }

    found_response.then(|| output.into_bytes())
}

fn decorate_tools_list_message(message: &mut Value, request_id: &Value) -> Option<()> {
    if message.get("jsonrpc")?.as_str()? != "2.0" || message.get("id")? != request_id {
        return None;
    }
    let tools = message
        .get_mut("result")?
        .get_mut("tools")?
        .as_array_mut()?;
    let schemes = json!([{"type":"oauth2","scopes":[CHATGPT_TOOL_SCOPE]}]);
    for tool in tools {
        let object = tool.as_object_mut()?;
        object.insert("securitySchemes".to_owned(), schemes.clone());
        let metadata = object
            .entry("_meta".to_owned())
            .or_insert_with(|| json!({}));
        if !metadata.is_object() {
            *metadata = json!({});
        }
        metadata
            .as_object_mut()?
            .insert("securitySchemes".to_owned(), schemes.clone());
    }
    Some(())
}

fn invalid_tools_list_response(mut parts: axum::http::response::Parts) -> Response {
    parts.status = StatusCode::BAD_GATEWAY;
    parts.headers.remove(header::CONTENT_LENGTH);
    parts
        .headers
        .insert(header::CONTENT_TYPE, HeaderValue::from_static("application/json"));
    parts
        .headers
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Response::from_parts(
        parts,
        Body::from(r#"{"code":"chatgpt_tool_metadata_unavailable"}"#),
    )
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let values = headers.get_all(header::AUTHORIZATION);
    let mut iter = values.iter();
    let value = iter.next()?;
    if iter.next().is_some() {
        return None;
    }
    let value = value.to_str().ok()?;
    let (scheme, token) = value.split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("Bearer")
        || token.is_empty()
        || token.starts_with(' ')
        || token.contains(char::is_whitespace)
        || token.contains(',')
    {
        return None;
    }
    Some(token)
}

fn origin_allowed(headers: &HeaderMap, allowed_origins: &[url::Url]) -> bool {
    let values = headers.get_all(header::ORIGIN);
    let mut iter = values.iter();
    let Some(value) = iter.next() else {
        return true;
    };
    if iter.next().is_some() {
        return false;
    }
    let Ok(value) = value.to_str() else {
        return false;
    };
    let Ok(origin) = url::Url::parse(value) else {
        return false;
    };
    if origin.path() != "/"
        || origin.query().is_some()
        || origin.fragment().is_some()
        || !origin.username().is_empty()
        || origin.password().is_some()
    {
        return false;
    }
    allowed_origins.iter().any(|allowed| {
        allowed.path() == "/"
            && allowed.query().is_none()
            && allowed.fragment().is_none()
            && allowed.origin() == origin.origin()
    })
}

fn is_spoofed_identity_header(name: &axum::http::HeaderName) -> bool {
    let name = name.as_str();
    name.starts_with("x-driichi-")
        || name.starts_with("x-mcp-")
        || name.starts_with("x-internal-")
        || name.starts_with("x-authenticated-")
        || name.starts_with("x-auth-request-")
        || name.starts_with("x-auth-")
        || name.starts_with("x-user-")
        || name.starts_with("x-remote-")
        || matches!(
            name,
            "remote-user"
                | "x-remote-user"
                | "x-forwarded-user"
                | "x-forwarded-email"
                | "x-user-id"
                | "x-user-email"
        )
}

fn unauthorized(resource: &url::Url, error: &'static str) -> Response {
    gateway_error(
        StatusCode::UNAUTHORIZED,
        error,
        Some(token_challenge(resource)),
    )
}

fn scope_challenge(resource: &url::Url) -> HeaderValue {
    challenge_value(
        resource,
        r#", error="insufficient_scope", scope="driichi:play""#,
    )
}

fn token_challenge(resource: &url::Url) -> HeaderValue {
    challenge_value(resource, r#", error="invalid_token""#)
}

fn challenge_value(resource: &url::Url, suffix: &str) -> HeaderValue {
    let mut metadata = resource.clone();
    metadata.set_path("/.well-known/oauth-protected-resource/chatgpt/mcp");
    metadata.set_query(None);
    metadata.set_fragment(None);
    HeaderValue::from_str(&format!(
        r#"Bearer resource_metadata="{}"{suffix}"#,
        metadata.as_str()
    ))
    .unwrap_or_else(|_| HeaderValue::from_static("Bearer"))
}

fn gateway_error(
    status: StatusCode,
    code: &'static str,
    challenge: Option<HeaderValue>,
) -> Response {
    let mut builder = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::CACHE_CONTROL, "no-store");
    if let Some(challenge) = challenge {
        builder = builder.header(header::WWW_AUTHENTICATE, challenge);
    }
    builder
        .body(Body::from(format!(r#"{{"code":"{code}"}}"#)))
        .expect("gateway error response is valid")
}
