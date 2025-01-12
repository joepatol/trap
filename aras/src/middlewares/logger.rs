use std::fmt::Debug;
use std::future::Future;
use std::pin::Pin;

use derive_more::derive::Constructor;
use tower::layer::Layer;
use hyper::service::Service;
use log::{info, error};

use crate::types::{Request, ResponseStatus};

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
    S: Service<Request>,
    <S as Service<Request>>::Future: Send + 'static,
    <S as Service<Request>>::Response: ResponseStatus,
    <S as Service<Request>>::Error: std::fmt::Display,
{
    type Error = S::Error;
    type Response = S::Response;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn call(&self, req: Request) -> Self::Future {
        info!("processing request: {} {}", req.method(), req.uri().path());
        let fut = self.inner.call(req);
        let fut = async {
            match fut.await {
                Ok(res) => {
                    info!("Response sent: {}", &res.status_string());
                    return Ok(res)
                },
                Err(e) => {
                    error!("Failed to send response: {}", e);
                    return Err(e)
                }
            }
        };
        Box::pin(fut)
    }
}

#[derive(Constructor)]
pub struct LogLayer;

impl<S> Layer<S> for LogLayer {
    type Service = Logger<S>;

    fn layer(&self, inner: S) -> Self::Service {
        Logger::new(inner)
    }
}