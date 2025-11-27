use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use asgispec::prelude::*;
use tower_http::compression::CompressionLayer;
use derive_more::derive::Constructor;
use hyper::server::conn::http1;
use hyper_util::rt::{TokioIo, TokioTimer};
use hyper_util::service::TowerToHyperService;
use log::{error, info};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

use super::middlewares::*;
use super::service::ArasASGIService;
use super::error::{Error, Result, UnexpectedShutdownSrc as SRC};
use super::protocols::LifespanHandler;
use super::types::ConnectionInfo;

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
    cancel_token: CancellationToken
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
                .layer(LogLayer::new())
                .load_shed()
                // .buffer(1024)
                .layer(CompressionLayer::new())
                .concurrency_limit(self.concurrency_limit)
                .timeout(self.timeout)
                .layer(RequestBodyLimitLayer::new(self.body_limit))
                .service(ArasASGIService::new(application.clone(), state.clone(), conn_info));
            let svc = TowerToHyperService::new(svc);

            tokio::task::spawn(async move {
                if let Err(err) = http1::Builder::new()
                    .timer(TokioTimer::new())
                    .keep_alive(keep_alive)
                    .serve_connection(io, svc)
                    .with_upgrades()
                    .await
                {
                    if err.is_closed() || err.is_timeout() {
                        info!("Disconnected client {client}");
                    } else {
                        error!("Error serving connection: {}", err);
                    };
                }
            });
        }
    }
}

impl<A> ASGIServer<A> for ArasServer
where
    A: ASGIApplication + 'static,
{
    type Output = Result<()>;

    async fn run(&self, application: A, state: A::State) -> Self::Output {
        let lifespan_handler = LifespanHandler::new()
        .startup(application.clone(), state.clone())
        .await?;

        tokio::select! {
            _ = self.cancel_token.cancelled() => lifespan_handler.shutdown().await,
            out = self.run_server(application, state) => out.map_err(|e| Error::unexpected_shutdown(SRC::Server, e.to_string())),
        }
    }
}