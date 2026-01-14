use std::future::Future;
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use asgispec::prelude::*;
use derive_more::derive::Constructor;
use hyper::server::conn::http1;
use hyper_util::rt::{TokioIo, TokioTimer};
use hyper_util::service::TowerToHyperService;
use log::{error, info};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tower_http::compression::CompressionLayer;
use tower_http::trace::TraceLayer;
use tower_http::ServiceBuilderExt;

use super::communication::CommunicationFactory;
use super::ArasResult;
use super::protocols::LifespanHandler;
use super::scope::ScopeFactory;
use super::service::ArasASGIService;
use super::types::{ConnectionInfo, Request, ResponseStatus};

/// The Aras server implementation
///
/// The server runs a tower service that uses a provided ASGI application to handle requests. The tower
/// service is run using hyper over a TCP listener.
/// The service is wrapped with various middleware, applied in the following order:
///
/// 1. TraceLayer for logging requests and responses
/// 2. CompressionLayer for response compression based on accept-encoding header
/// 3. Request body size limit to reject requests with bodies that are too large
/// 4. Load shedding to reject requests when the server is overloaded
/// 5. Buffering to queue requests when the server is busy
/// 6. Rate limiting to limit the number of requests per time period
/// 7. Concurrency limiting to limit the number of in-flight requests
/// 8. Timeout to limit the total time for processing a request
///
/// This way requests are only fully rejected once the concurrency limit and buffer are fully utilized, or the rate limit is exceeded.
/// Further middleware can be applied in the ASGI application itself. This middleware stack is intentionally left minimal.
/// It ensures the server can handle high loads gracefully while protecting resources, it is fully configurable through the server settings.
#[derive(Constructor, Clone)]
pub struct ArasServer {
    /// Cancellation token to stop the server
    cancel_token: CancellationToken,
    /// Host address
    addr: IpAddr,
    /// Host port
    port: u16,
    /// Whether to use HTTP keep-alive
    keep_alive: bool,
    /// Request timeout. If the innermost ASGI service fails to respond after this time
    /// (the entire request lifecycle), the server will return a 504 Gateway Timeout response.
    request_timeout: Duration,
    /// Max request body size. If the size is exceeded, the server will return a 413 Payload Too Large response.
    body_limit: usize,
    /// Max number of in-flight requests. If the limit is reached, requests will be
    /// rejected with a 503 Service Unavailable response.
    concurrency_limit: usize,
    /// Rate limit, as (requests, duration). If more requests are received in the duration,
    /// they will be rejected with a 429 response.
    rate_limit: (u64, Duration),
    /// Maximum buffer size for requests. Indicates the number of requests that will be buffered
    /// after max concurrency and/or rate limits have been reached.
    buffer_size: usize,
    /// Number of seconds the server will wait for events like reading the incoming body stream
    /// or receiving ASGI messages from the application.
    /// Allows for more granular control than the general request timeout
    backpressure_timeout: u64,
    /// The max size of a single websocket frame in bytes. If a frame larger it will be fragmented.
    max_ws_frame_size: usize,
}

impl<A> ASGIServer<A> for ArasServer
where
    A: ASGIApplication + 'static,
{
    type Output = ArasResult<()>;

    async fn run(&self, application: A, state: A::State) -> Self::Output {
        let timeout = Duration::from_secs(self.backpressure_timeout);
        let scope_factory = ScopeFactory::new(state);
        let communication_factory = CommunicationFactory::new(application);

        let scope = scope_factory.build_lifespan();
        let (send_to_app, receive_from_app) = communication_factory.build(scope);
        let mut lifespan_handler = LifespanHandler::new(timeout)
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

            let svc = tower::ServiceBuilder::new()
                .layer(
                    TraceLayer::new_for_http()
                        .on_request(|req: &Request, _span: &tracing::Span| {
                            info!("Processing request: {} {}", req.method(), req.uri().path())
                        })
                        .on_response(|res: &http::Response<_>, _latency: Duration, _span: &tracing::Span| {
                            info!("Response sent: {}", res.status_string())
                        }),
                )
                .layer(CompressionLayer::new())
                .request_body_limit(self.body_limit)
                .load_shed()
                .buffer(self.buffer_size)
                .rate_limit(self.rate_limit.0, self.rate_limit.1)
                .concurrency_limit(self.concurrency_limit)
                .timeout(self.request_timeout)
                .service(ArasASGIService::new_from_factories(
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
                .auto_date_header(true)
                .serve_connection(io, svc)
                .with_upgrades();

            let handled = log_hyper_error(client, conn);
            tokio::task::spawn(handled);
        }
    }
}

async fn log_hyper_error(client: SocketAddr, conn: impl Future<Output = std::result::Result<(), hyper::Error>>) {
    if let Err(e) = conn.await {
        if e.is_closed() | e.is_canceled() {
            error!("Connection closed by client: {}. \n {}", client, e);
        } else {
            error!("Failed to serve connection: {}. \n {}", client, e);
        }
    }
}
