use std::{
    env,
    ffi::OsStr,
    fmt,
    future::Future,
    process::ExitCode,
    sync::{Arc, OnceLock},
};

use rmcp::{
    ClientHandler, ErrorData, ServerHandler, ServiceError,
    model::{
        CallToolRequestParams, CallToolResponse, ClientCapabilities, ClientConfig, Implementation,
        ListResourceTemplatesResult, ListResourcesResult, ListToolsResult,
        ReadResourceRequestParams, ReadResourceResponse, ServerConfig, SubscribeRequestParams,
        UnsubscribeRequestParams,
    },
    service::{NotificationContext, RequestContext, RoleClient, RoleServer, ServiceExt},
    transport::{
        StreamableHttpClientTransport, stdio,
        streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use url::Url;

const MAX_HTTP_CONCURRENT_REQUESTS: usize = 8;
const MAX_SSE_EVENT_SIZE: usize = 1024 * 1024;

/// The validated command-line configuration for one bridge process.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BridgeConfig {
    pub server: Url,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    Usage,
    InvalidServerUrl,
    MissingToken,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage => f.write_str("usage: driichi-mcp --server URL"),
            Self::InvalidServerUrl => f.write_str("invalid MCP server URL"),
            Self::MissingToken => f.write_str("DRIICHI_MCP_TOKEN is required"),
        }
    }
}

impl std::error::Error for ConfigError {}

/// Parse and validate the bridge's intentionally small CLI surface.
pub fn parse_args<I, S>(args: I) -> Result<BridgeConfig, ConfigError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let args: Vec<_> = args.into_iter().collect();
    if args.len() != 3 || args[1].as_ref() != OsStr::new("--server") {
        return Err(ConfigError::Usage);
    }
    let raw_url = args[2]
        .as_ref()
        .to_str()
        .ok_or(ConfigError::InvalidServerUrl)?;
    let server = Url::parse(raw_url).map_err(|_| ConfigError::InvalidServerUrl)?;
    if !matches!(server.scheme(), "http" | "https")
        || server.host_str().is_none()
        || raw_url.contains('@')
        || !server.username().is_empty()
        || server.password().is_some()
        || server.query().is_some()
        || server.fragment().is_some()
        || server.path() != "/mcp"
    {
        return Err(ConfigError::InvalidServerUrl);
    }
    Ok(BridgeConfig { server })
}

fn token_from_environment() -> Result<String, ConfigError> {
    match env::var("DRIICHI_MCP_TOKEN") {
        Ok(token) if !token.is_empty() => Ok(token),
        _ => Err(ConfigError::MissingToken),
    }
}

#[derive(Debug)]
pub enum BridgeError {
    Config(ConfigError),
    Upstream,
    Downstream,
}

impl fmt::Display for BridgeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(error) => error.fmt(f),
            Self::Upstream => f.write_str("MCP upstream connection failed"),
            Self::Downstream => f.write_str("MCP stdio connection failed"),
        }
    }
}

impl std::error::Error for BridgeError {}

#[derive(Clone)]
struct UpstreamClient {
    downstream: Arc<OnceLock<rmcp::Peer<RoleServer>>>,
}

impl UpstreamClient {
    fn new(downstream: Arc<OnceLock<rmcp::Peer<RoleServer>>>) -> Self {
        Self { downstream }
    }
}

#[allow(clippy::manual_async_fn)]
impl ClientHandler for UpstreamClient {
    fn on_resource_updated(
        &self,
        params: rmcp::model::ResourceUpdatedNotificationParam,
        _context: NotificationContext<RoleClient>,
    ) -> impl Future<Output = ()> + rmcp::service::MaybeSendFuture + '_ {
        async move {
            if let Some(peer) = self.downstream.get() {
                let _ = peer.notify_resource_updated(params).await;
            }
        }
    }

    fn on_resource_list_changed(
        &self,
        _context: NotificationContext<RoleClient>,
    ) -> impl Future<Output = ()> + rmcp::service::MaybeSendFuture + '_ {
        async move {
            if let Some(peer) = self.downstream.get() {
                let _ = peer.notify_resource_list_changed().await;
            }
        }
    }

    fn on_tool_list_changed(
        &self,
        _context: NotificationContext<RoleClient>,
    ) -> impl Future<Output = ()> + rmcp::service::MaybeSendFuture + '_ {
        async move {
            if let Some(peer) = self.downstream.get() {
                let _ = peer.notify_tool_list_changed().await;
            }
        }
    }

    fn get_info(&self) -> ClientConfig {
        ClientConfig::new(
            ClientCapabilities::default(),
            Implementation::new(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION")),
        )
    }
}

struct ProxyServer {
    upstream: rmcp::Peer<RoleClient>,
    downstream: Arc<OnceLock<rmcp::Peer<RoleServer>>>,
    info: ServerConfig,
}

impl ProxyServer {
    fn new(
        upstream: rmcp::Peer<RoleClient>,
        downstream: Arc<OnceLock<rmcp::Peer<RoleServer>>>,
        info: ServerConfig,
    ) -> Self {
        Self {
            upstream,
            downstream,
            info,
        }
    }

    fn upstream_error(_error: ServiceError) -> ErrorData {
        ErrorData::internal_error("MCP upstream request failed", None)
    }
}

#[allow(clippy::manual_async_fn)]
impl ServerHandler for ProxyServer {
    fn ping(
        &self,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<(), ErrorData>> + rmcp::service::MaybeSendFuture + '_ {
        async move {
            let result = self
                .upstream
                .send_request(
                    rmcp::model::PingRequest {
                        method: Default::default(),
                        extensions: Default::default(),
                    }
                    .into(),
                )
                .await
                .map_err(Self::upstream_error)?;
            match result {
                rmcp::model::ServerResult::EmptyResult(_) => Ok(()),
                _ => Err(ErrorData::internal_error(
                    "MCP upstream request failed",
                    None,
                )),
            }
        }
    }

    fn list_tools(
        &self,
        request: Option<rmcp::model::PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListToolsResult, ErrorData>> + rmcp::service::MaybeSendFuture + '_
    {
        async move {
            self.upstream
                .list_tools(request)
                .await
                .map_err(Self::upstream_error)
        }
    }

    fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<CallToolResponse, ErrorData>> + rmcp::service::MaybeSendFuture + '_
    {
        async move {
            self.upstream
                .call_tool(request)
                .await
                .map(CallToolResponse::Complete)
                .map_err(Self::upstream_error)
        }
    }

    fn list_resources(
        &self,
        request: Option<rmcp::model::PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListResourcesResult, ErrorData>> + rmcp::service::MaybeSendFuture + '_
    {
        async move {
            self.upstream
                .list_resources(request)
                .await
                .map_err(Self::upstream_error)
        }
    }

    fn list_resource_templates(
        &self,
        request: Option<rmcp::model::PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListResourceTemplatesResult, ErrorData>>
    + rmcp::service::MaybeSendFuture
    + '_ {
        async move {
            self.upstream
                .list_resource_templates(request)
                .await
                .map_err(Self::upstream_error)
        }
    }

    fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ReadResourceResponse, ErrorData>>
    + rmcp::service::MaybeSendFuture
    + '_ {
        async move {
            self.upstream
                .read_resource(request)
                .await
                .map(ReadResourceResponse::Complete)
                .map_err(Self::upstream_error)
        }
    }

    #[allow(deprecated)]
    fn subscribe(
        &self,
        request: SubscribeRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<(), ErrorData>> + rmcp::service::MaybeSendFuture + '_ {
        async move {
            self.upstream
                .subscribe(request)
                .await
                .map_err(Self::upstream_error)
        }
    }

    #[allow(deprecated)]
    fn unsubscribe(
        &self,
        request: UnsubscribeRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<(), ErrorData>> + rmcp::service::MaybeSendFuture + '_ {
        async move {
            self.upstream
                .unsubscribe(request)
                .await
                .map_err(Self::upstream_error)
        }
    }

    fn on_initialized(
        &self,
        context: NotificationContext<RoleServer>,
    ) -> impl Future<Output = ()> + rmcp::service::MaybeSendFuture + '_ {
        let _ = self.downstream.set(context.peer.clone());
        std::future::ready(())
    }

    fn get_info(&self) -> ServerConfig {
        self.info.clone()
    }
}

fn server_config(info: &rmcp::model::ServerPeerInfo) -> ServerConfig {
    let mut config = ServerConfig::new(info.capabilities.clone())
        .with_protocol_version(info.protocol_version.clone());
    if let Some(server_info) = info.server_info.clone() {
        config = config.with_server_info(server_info);
    }
    if let Some(instructions) = info.instructions.clone() {
        config = config.with_instructions(instructions);
    }
    config
}

/// Run one authenticated HTTP-to-stdio bridge.
pub async fn run() -> Result<(), BridgeError> {
    let config = parse_args(env::args_os()).map_err(BridgeError::Config)?;
    let token = token_from_environment().map_err(BridgeError::Config)?;
    let downstream = Arc::new(OnceLock::new());
    let transport_config = StreamableHttpClientTransportConfig::with_uri(config.server.to_string())
        .auth_header(token)
        .max_concurrent_requests(MAX_HTTP_CONCURRENT_REQUESTS)
        .max_sse_event_size(MAX_SSE_EVENT_SIZE);
    let upstream_transport = StreamableHttpClientTransport::from_config(transport_config);
    let upstream = rmcp::serve_client(UpstreamClient::new(downstream.clone()), upstream_transport)
        .await
        .map_err(|_| BridgeError::Upstream)?;
    let upstream_info = upstream.peer_info().ok_or(BridgeError::Upstream)?;
    let proxy = ProxyServer::new(
        upstream.peer().clone(),
        downstream,
        server_config(&upstream_info),
    );
    let local = proxy
        .serve(stdio())
        .await
        .map_err(|_| BridgeError::Downstream)?;
    let upstream_cancel = upstream.cancellation_token();
    let local_cancel = local.cancellation_token();
    tokio::select! {
        result = local.waiting() => {
            upstream_cancel.cancel();
            result.map(|_| ()).map_err(|_| BridgeError::Downstream)
        }
        result = upstream.waiting() => {
            local_cancel.cancel();
            result.map(|_| ()).map_err(|_| BridgeError::Upstream)
        }
    }
}

/// Entry-point helper that keeps process errors free of credentials.
pub async fn process_entrypoint() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(url: &str) -> [&str; 3] {
        ["driichi-mcp", "--server", url]
    }

    #[test]
    fn accepts_only_mcp_http_urls() {
        let config = parse_args(args("https://example.test/mcp")).unwrap();
        assert_eq!(config.server.as_str(), "https://example.test/mcp");
        assert!(parse_args(args("http://127.0.0.1:3000/mcp")).is_ok());
    }

    #[test]
    fn rejects_bad_arguments_and_url_components() {
        for input in [
            vec!["driichi-mcp"],
            vec!["driichi-mcp", "--server"],
            vec!["driichi-mcp", "--other", "https://example.test/mcp"],
            vec![
                "driichi-mcp",
                "--server",
                "https://example.test/mcp",
                "extra",
            ],
        ] {
            assert_eq!(parse_args(input), Err(ConfigError::Usage));
        }
        for url in [
            "ftp://example.test/mcp",
            "https://@example.test/mcp",
            "https://user@example.test/mcp",
            "https://user:pass@example.test/mcp",
            "https://example.test/mcp?token=secret",
            "https://example.test/mcp#fragment",
            "https://example.test/other",
            "https://example.test/",
        ] {
            assert_eq!(
                parse_args(args(url)),
                Err(ConfigError::InvalidServerUrl),
                "{url}"
            );
        }
    }
}
