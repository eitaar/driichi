use std::{str::FromStr, sync::Arc};

use axum::{
    Extension,
    body::Body,
    extract::State,
    http::{HeaderMap, HeaderValue, Request, StatusCode, header},
    response::Response,
};
use zeroize::Zeroizing;

use crate::{mcp::McpRuntime, oauth::OAuthError, ServerState};

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
            && std::env::var("DRIICHI_CHATGPT_BOT_TOKEN").ok().as_deref()
                != Some(self.raw.as_str())
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
    let Some(oauth) = state.chatgpt_oauth.as_ref() else {
        return gateway_error(StatusCode::SERVICE_UNAVAILABLE, "oauth_unavailable", None);
    };

    if request.headers().keys().any(is_spoofed_identity_header) {
        return gateway_error(StatusCode::FORBIDDEN, "spoofed_identity_header", None);
    }
    if !origin_allowed(request.headers(), &oauth.config.allowed_origins) {
        return gateway_error(StatusCode::FORBIDDEN, "origin_not_allowed", None);
    }

    let Some(access_token) = bearer_token(request.headers()) else {
        return unauthorized(&oauth.config.resource, "invalid_token");
    };
    let grant = match oauth
        .grants
        .validate_access(access_token, oauth.config.resource.as_str(), "driichi:play")
        .await
    {
        Ok(grant) => grant,
        Err(OAuthError::InsufficientScope) => {
            return gateway_error(
                StatusCode::FORBIDDEN,
                "insufficient_scope",
                Some(scope_challenge(&oauth.config.resource)),
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
        return unauthorized(&oauth.config.resource, "invalid_token");
    }

    let Some(token) = state.chatgpt_bot_token.as_ref() else {
        return gateway_error(StatusCode::SERVICE_UNAVAILABLE, "delegation_unavailable", None);
    };
    if !token.is_active(&state) {
        return gateway_error(StatusCode::SERVICE_UNAVAILABLE, "delegation_unavailable", None);
    }
    let authorization = format!("Bearer {}", token.raw.as_str());
    let internal_authorization = match HeaderValue::from_str(&authorization) {
        Ok(value) => value,
        Err(_) => {
            return gateway_error(StatusCode::SERVICE_UNAVAILABLE, "delegation_unavailable", None);
        }
    };

    request.headers_mut().remove(header::AUTHORIZATION);
    request.headers_mut().remove(header::ORIGIN);
    request
        .headers_mut()
        .insert(header::AUTHORIZATION, internal_authorization);
    runtime.handle(state, request).await
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
    challenge_value(resource, r#", error="insufficient_scope", scope="driichi:play""#)
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
