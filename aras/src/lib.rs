mod types;
mod error;
mod http;
mod lifespan;
mod server;
mod websocket;
mod application;
mod middlewares;

use asgispec::prelude::*;

pub use crate::error::{Error, Result};
pub use crate::application::{Application, ApplicationFactory};
pub use crate::server::{Server, ServerConfig};

pub async fn serve<S: State + 'static, T: ASGIApplication<S> + 'static>(app: T, state: S, config: Option<ServerConfig>) -> Result<()> {
    let mut server = Server::new(app, state);
    server.serve(config.unwrap_or_default()).await?;
    Ok(())
}
