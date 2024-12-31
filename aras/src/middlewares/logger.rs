use std::{fmt::Debug, sync::Arc};

use hyper::service::Service;
use hyper::Request;
use hyper::body::Incoming;
use log::{info, error};

use crate::types::{Response, ServiceFuture};
use crate::error::Error;

#[derive(Debug, Clone)]
pub struct Logger<S> {
    inner: Arc<S>,
}

impl<S> Logger<S> {
    pub fn new(inner: S) -> Self {
        Logger { inner: Arc::new(inner) }
    }
}

impl<S> Service<Request<Incoming>> for Logger<S>
where
    S: Service<
    Request<Incoming>, 
        Response = Response,
        Error = Error, 
        Future = ServiceFuture,
    > + Send + Sync + 'static,
{
    type Error = S::Error;
    type Response = S::Response;
    type Future = S::Future;

    fn call(&self, req: Request<Incoming>) -> Self::Future {
        info!("processing request: {} {}", req.method(), req.uri().path());
        let inner_clone = self.inner.clone();
        Box::pin(async move {
            match inner_clone.call(req).await {
                Ok(res) => {
                    info!("Response sent: {}", &res.status());
                    Ok(res)
                },
                Err(e) => {
                    error!("Failed to send response: {}", e);
                    Err(e)
                }
            }
        })
    }
}
