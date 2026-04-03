use std::future::Future;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use asgispec::prelude::*;
use http::HeaderName;
use hyper::server::conn::http1;
use hyper_util::rt::{TokioIo, TokioTimer};
use hyper_util::service::TowerToHyperService;
use log::{error, info};
use tokio::net::TcpListener;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use tower::Layer;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::compression::CompressionLayer;
use tower_http::decompression::RequestDecompressionLayer;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::sensitive_headers::SetSensitiveRequestHeadersLayer;
use tower_http::trace::TraceLayer;
use tower_http::ServiceBuilderExt;

use super::communication::CommunicationFactory;
use super::layers::ErrorHandlerLayer;
use super::protocols::LifespanHandler;
use super::scope::ScopeFactory;
use super::service::ArasASGIService;
use super::types::{ConnectionInfo, Request, ResponseStatus};
use super::ArasResult;

/// The Aras server implementation
///
/// The server runs a tower service that uses a provided ASGI application to handle requests. The tower
/// service is run using hyper over a TCP listener.
/// The service is wrapped with various middleware, applied in the following order (if enabled):
///
/// 1.  CatchPanicLayer       — converts handler panics into 500 responses instead of closing the connection
/// 2.  SetRequestIdLayer     — assigns a unique `x-request-id` header to every request
/// 3.  TraceLayer            — logs requests and responses, propagating the request id into spans
/// 4.  PropagateRequestIdLayer — copies `x-request-id` onto the response for clients to correlate
/// 5.  SetSensitiveRequestHeadersLayer — redacts configured headers from trace logs
/// 6.  ErrorHandlerLayer     — converts all stack errors into appropriate HTTP responses (503/504/500)
/// 7.  CompressionLayer      — compresses responses based on Accept-Encoding
/// 8.  RequestDecompressionLayer — decompresses request bodies based on Content-Encoding
/// 9.  Request body size limit — rejects oversized bodies with 413
/// 10. Load shedding         — rejects requests when the buffer is full
/// 11. Buffering             — queues requests when concurrency/rate limits are reached
/// 12. Rate limiting         — rejects excess requests with 429
/// 13. Concurrency limiting  — limits in-flight requests
/// 14. Timeout               — returns 504 if the request lifecycle exceeds the configured duration
///
/// This way requests are only fully rejected once the concurrency limit and buffer are fully utilized, or the rate limit is exceeded.
/// Further middleware can be applied in the ASGI application itself. This middleware stack is intentionally left minimal.
/// It ensures the server can handle high loads gracefully while protecting resources, it is fully configurable through the server settings.
///
/// Use [`ArasServer::builder`] to construct an instance.
pub struct ArasServer {
    cancel_token: CancellationToken,
    addr: IpAddr,
    port: u16,
    keep_alive: bool,
    request_timeout: Duration,
    body_limit: usize,
    concurrency_limit: usize,
    rate_limit: (u64, Duration),
    buffer_size: usize,
    backpressure_timeout: Duration,
    backpressure_size: usize,
    max_ws_frame_size: usize,
    request_ids: bool,
    auto_date_header: bool,
    sensitive_headers: Vec<HeaderName>,
}

impl ArasServer {
    /// Returns a builder for [`ArasServer`].
    ///
    /// Calling `cancel()` on the provided `CancellationToken` will trigger a graceful shutdown.
    pub fn builder(cancel_token: CancellationToken) -> ArasServerBuilder {
        ArasServerBuilder::new(cancel_token)
    }
}

/// Builder for [`ArasServer`].
///
/// Obtain one via [`ArasServer::builder`].
///
/// Optional middleware layers are disabled by default and enabled by calling their respective
/// methods. Required layers (`TraceLayer`, `ErrorHandlerLayer`) are always active.
pub struct ArasServerBuilder {
    cancel_token: CancellationToken,
    addr: IpAddr,
    port: u16,
    keep_alive: bool,
    request_timeout: Duration,
    body_limit: usize,
    concurrency_limit: usize,
    rate_limit: (u64, Duration),
    buffer_size: usize,
    backpressure_timeout: Duration,
    backpressure_size: usize,
    max_ws_frame_size: usize,
    request_ids: bool,
    auto_date_header: bool,
    sensitive_headers: Vec<HeaderName>,
}

impl ArasServerBuilder {
    fn new(cancel_token: CancellationToken) -> Self {
        Self {
            cancel_token,
            addr: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 8080,
            keep_alive: true,
            request_timeout: Duration::from_secs(180),
            body_limit: 1_000_000_000,
            concurrency_limit: Semaphore::MAX_PERMITS,
            rate_limit: (1000, Duration::from_secs(1)),
            buffer_size: 1024,
            backpressure_timeout: Duration::from_secs(60),
            backpressure_size: 16,
            max_ws_frame_size: 64 * 1024,
            request_ids: false,
            auto_date_header: true,
            sensitive_headers: Vec::new(),
        }
    }

    /// Host address to bind to. Defaults to `127.0.0.1`.
    pub fn addr(mut self, addr: IpAddr) -> Self {
        self.addr = addr;
        self
    }

    /// Port to listen on. Defaults to `8080`.
    pub fn port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    /// Disable HTTP keep-alive.
    pub fn no_keep_alive(mut self) -> Self {
        self.keep_alive = false;
        self
    }

    /// Maximum time for the entire request lifecycle before returning `504 Gateway Timeout`.
    /// Defaults to 180 seconds.
    pub fn request_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = timeout;
        self
    }

    /// Maximum request body size in bytes. Requests exceeding this return `413 Payload Too Large`.
    /// Defaults to 1 GB.
    pub fn body_limit(mut self, limit: usize) -> Self {
        self.body_limit = limit;
        self
    }

    /// Maximum number of in-flight requests. Requests beyond this limit return `503 Service Unavailable`.
    /// Defaults to effectively unlimited.
    pub fn concurrency_limit(mut self, limit: usize) -> Self {
        self.concurrency_limit = limit;
        self
    }

    /// Rate limit as `(max_requests, window_seconds)`. Requests exceeding the rate return `429`.
    /// Defaults to `(1000, 1)`.
    pub fn rate_limit(mut self, requests: u64, per_seconds: u64) -> Self {
        self.rate_limit = (requests, Duration::from_secs(per_seconds));
        self
    }

    /// Number of requests to buffer once the concurrency limit is reached.
    /// Defaults to `1024`.
    pub fn buffer_size(mut self, size: usize) -> Self {
        self.buffer_size = size;
        self
    }

    /// Maximum duration to wait when reading the request body or receiving ASGI messages from the
    /// application. Provides finer-grained control than `request_timeout`.
    /// Defaults to 60 seconds.
    pub fn backpressure_timeout(mut self, timeout: Duration) -> Self {
        self.backpressure_timeout = timeout;
        self
    }

    /// Number of ASGI messages to buffer when the application is producing messages faster than the server can send them to the client.
    /// Defaults to `16`.
    pub fn backpressure_size(mut self, size: usize) -> Self {
        self.backpressure_size = size;
        self
    }

    /// Maximum WebSocket frame size in bytes. Larger payloads are fragmented.
    /// Defaults to 64 KiB.
    pub fn max_ws_frame_size(mut self, size: usize) -> Self {
        self.max_ws_frame_size = size;
        self
    }

    /// Assign a unique `x-request-id` header to every request and propagate it onto the response.
    /// Useful for correlating log lines across a request lifecycle.
    pub fn request_ids(mut self) -> Self {
        self.request_ids = true;
        self
    }

    /// Disable automatic `Date` header on responses.
    pub fn disable_auto_date_header(mut self) -> Self {
        self.auto_date_header = false;
        self
    }

    /// Redact a header from trace logs.
    pub fn sensitive_header(mut self, header: HeaderName) -> Self {
        self.sensitive_headers.push(header);
        self
    }

    /// Builds the [`ArasServer`].
    pub fn build(self) -> ArasServer {
        ArasServer {
            cancel_token: self.cancel_token,
            addr: self.addr,
            port: self.port,
            keep_alive: self.keep_alive,
            request_timeout: self.request_timeout,
            body_limit: self.body_limit,
            concurrency_limit: self.concurrency_limit,
            rate_limit: self.rate_limit,
            buffer_size: self.buffer_size,
            backpressure_timeout: self.backpressure_timeout,
            backpressure_size: self.backpressure_size,
            max_ws_frame_size: self.max_ws_frame_size,
            request_ids: self.request_ids,
            auto_date_header: self.auto_date_header,
            sensitive_headers: self.sensitive_headers,
        }
    }
}

impl<A> ASGIServer<A> for ArasServer
where
    A: ASGIApplication + 'static,
{
    type Output = ArasResult<()>;

    async fn run(&self, application: A, state: A::State) -> Self::Output {
        let scope_factory = ScopeFactory::new(state);
        let communication_factory = CommunicationFactory::new(application, self.backpressure_size);

        let scope = scope_factory.build_lifespan();
        let (send_to_app, receive_from_app) = communication_factory.build(scope);
        let mut lifespan_handler = LifespanHandler::new(self.backpressure_timeout)
            .startup(send_to_app, receive_from_app)
            .await?;

        tokio::select! {
            _ = self.cancel_token.cancelled() => lifespan_handler.shutdown().await,
            out = self.run_server(scope_factory, communication_factory) => out,
        }
    }
}

impl ArasServer {
    async fn run_server<A: ASGIApplication + 'static>(
        &self,
        scope_factory: ScopeFactory<A::State>,
        communication_factory: CommunicationFactory<A>,
    ) -> ArasResult<()> {
        let keep_alive = self.keep_alive;
        let socket_addr = SocketAddr::new(self.addr, self.port);
        let listener = TcpListener::bind(socket_addr).await.expect("Failed to bind socket");
        info!("Listening on http://{}", socket_addr);

        let x_request_id = http::HeaderName::from_static("x-request-id");

        let stack = tower::ServiceBuilder::new()
            .layer(CatchPanicLayer::new())
            .option_layer(
                self.request_ids
                    .then(|| SetRequestIdLayer::new(x_request_id.clone(), MakeRequestUuid)),
            )
            .layer(
                TraceLayer::new_for_http()
                    .on_request(|req: &Request, _span: &tracing::Span| {
                        info!("Processing request: {} {}", req.method(), req.uri().path())
                    })
                    .on_response(|res: &http::Response<_>, _latency: Duration, _span: &tracing::Span| {
                        info!("Response sent: {}", res.status_string())
                    }),
            )
            .option_layer(self.request_ids.then(|| PropagateRequestIdLayer::new(x_request_id)))
            .option_layer(
                (self.sensitive_headers.len() > 0)
                    .then(|| SetSensitiveRequestHeadersLayer::new(self.sensitive_headers.clone())),
            )
            .layer(ErrorHandlerLayer::new())
            .layer(CompressionLayer::new())
            .layer(RequestDecompressionLayer::new())
            .request_body_limit(self.body_limit)
            .load_shed()
            .buffer(self.buffer_size)
            .rate_limit(self.rate_limit.0, self.rate_limit.1)
            .concurrency_limit(self.concurrency_limit)
            .timeout(self.request_timeout)
            .into_inner();

        loop {
            let (tcp, client) = match listener.accept().await {
                Ok((t, c)) => (t, c),
                Err(e) => {
                    error!("Failed to connect to client: {e}");
                    continue;
                }
            };

            let conn_info = ConnectionInfo::new(client, socket_addr);
            let io = TokioIo::new(tcp);
            info!("Connecting new client {client}");

            let svc = stack.layer(ArasASGIService::new_from_factories(
                scope_factory.clone(),
                communication_factory.clone(),
                conn_info,
                self.backpressure_timeout,
                self.max_ws_frame_size,
            ));

            let svc = TowerToHyperService::new(svc);
            let conn = http1::Builder::new()
                .timer(TokioTimer::new())
                .keep_alive(keep_alive)
                .auto_date_header(self.auto_date_header)
                .serve_connection(io, svc)
                .with_upgrades();

            let handled = handle_conn_close(client, conn);
            tokio::task::spawn(handled);
        }
    }
}

async fn handle_conn_close(client: SocketAddr, conn: impl Future<Output = std::result::Result<(), hyper::Error>>) {
    if let Err(e) = conn.await {
        if e.is_closed() || e.is_canceled() {
            info!("Connection closed by client: {}. \n {}", client, e);
        } else {
            error!("Failed to serve connection: {}. \n {}", client, e);
        }
    }
}
