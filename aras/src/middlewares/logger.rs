use std::future::Future;
use std::pin::Pin;
use std::fmt::Debug;

use hyper::service::Service;
use log::{info, error};

use crate::types::{Response, Request};
use crate::error::Error;

#[derive(Debug, Clone)]
pub struct Logger<S> {
    inner: S,
}

impl<S> Logger<S> {
    pub fn new(inner: S) -> Self {
        Logger { inner }
    }
}

impl<S> Service<Request> for Logger<S>
where
    S: Service<
        Request, 
        Response = Response,
        Error = Error, 
        Future = Pin<Box<dyn Future<Output = Result<Response, Error>>>>,
    > + Send + Sync + 'static,
{
    type Error = S::Error;
    type Response = S::Response;
    type Future = S::Future;

    fn call(&self, req: Request) -> Self::Future {
        info!("processing request: {} {}", req.method(), req.uri().path());
        let fut = self.inner.call(req);
        Box::pin(async move {
            match fut.await {
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