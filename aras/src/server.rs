use std::net::{IpAddr, SocketAddr};

use asgispec::prelude::*;
use derive_more::derive::Constructor;
use hyper::server::conn::http1;
use hyper_util::rt::{TokioIo, TokioTimer};
use log::{error, info};
use tokio::net::TcpListener;

use super::middlewares::Logger;
use super::service::aras_asgi_service;
use super::error::{Error, Result, UnexpectedShutdownSrc as SRC};
use super::protocols::LifespanHandler;
use super::types::ConnectionInfo;

/// Generic server that serves some `hyper::service::Service`.
#[derive(Constructor)]
pub struct ArasServer {
    /// Host address
    addr: IpAddr,
    /// Host port
    port: u16,
    /// Whether to use HTTP keep-alive
    keep_alive: bool,
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

            let svc = hyper::service::service_fn(move |req| {
                aras_asgi_service(req, application.clone(), conn_info.clone(), state.clone())
            });
            let svc = tower::ServiceBuilder::new()
                .layer_fn(Logger::new)
                .service(svc);

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
            _ = tokio::signal::ctrl_c() => lifespan_handler.shutdown().await,
            out = self.run_server(application, state) => out.map_err(|e| Error::unexpected_shutdown(SRC::Server, e.to_string())),
        }
    }
}
