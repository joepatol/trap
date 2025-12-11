use std::future::Future;
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use asgispec::prelude::*;
use tower_http::ServiceBuilderExt;
use tower_http::compression::CompressionLayer;
use derive_more::derive::Constructor;
use hyper::server::conn::http1;
use hyper_util::rt::{TokioIo, TokioTimer};
use hyper_util::service::TowerToHyperService;
use log::{error, info};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tower_http::trace::TraceLayer;

use crate::types::ResponseStatus;

use super::service::ArasASGIService;
use super::error::{Error, Result, UnexpectedShutdownSrc as SRC};
use super::protocols::LifespanHandler;
use super::types::{ConnectionInfo, Request};

async fn handle_hyper_conn_error(conn: impl Future<Output = std::result::Result<(), hyper::Error>>) {
    if let Err(e) = conn.await {
        error!("Failed to serve connection: {}", e);
    }
}

/// The Aras server implementation
#[derive(Constructor, Clone)]
pub struct ArasServer {
    /// Host address
    addr: IpAddr,
    /// Host port
    port: u16,
    /// Whether to use HTTP keep-alive
    keep_alive: bool,
    /// Request timeout
    timeout: Duration,
    /// Max request body size
    body_limit: usize,
    /// Max number of in-flight requests
    concurrency_limit: usize,
    /// Cancellation token to stop the server
    cancel_token: CancellationToken,
    /// Rate limit
    rate_limit: (u64, Duration),
    /// Maximum buffer size for requests
    buffer_size: usize,
    /// Number of seconds the server will wait for an expected ASGI event
    asgi_timeout_secs: u64,
}

impl ArasServer {
    async fn run_server<A: ASGIApplication + 'static>(&self, application: A, state: A::State) -> Result<()> {
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

            let application = application.clone();
            let state = state.clone();
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
                        })
                )
                .request_body_limit(self.body_limit)
                .layer(CompressionLayer::new())
                .buffer(self.buffer_size)
                .rate_limit(self.rate_limit.0, self.rate_limit.1)
                .concurrency_limit(self.concurrency_limit)
                .load_shed()
                .timeout(self.timeout)
                .service(ArasASGIService::new(application.clone(), state.clone(), conn_info));

            let svc = TowerToHyperService::new(svc);
            let conn = http1::Builder::new()
                    .timer(TokioTimer::new())
                    .keep_alive(keep_alive)
                    .auto_date_header(true)
                    .serve_connection(io, svc)
                    .with_upgrades();

            let handled = handle_hyper_conn_error(conn);
            tokio::task::spawn(handled);
        }
    }
}

impl<A> ASGIServer<A> for ArasServer
where
    A: ASGIApplication + 'static,
{
    type Output = Result<()>;

    async fn run(&self, application: A, state: A::State) -> Self::Output {
        let lifespan_handler = LifespanHandler::new(self.asgi_timeout_secs)
        .startup(application.clone(), state.clone())
        .await?;

        tokio::select! {
            _ = self.cancel_token.cancelled() => lifespan_handler.shutdown().await,
            out = self.run_server(application, state) => out.map_err(|e| Error::unexpected_shutdown(SRC::Server, e.to_string())),
        }
    }
}
