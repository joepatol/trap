use std::fmt::Debug;
use std::sync::Arc;

use derive_more::derive::Constructor;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::service::Service;
use hyper::Request;
use tokio::sync::Semaphore;

use crate::error::{Error, Result};
use crate::types::{Response, ServiceFuture};

#[derive(Constructor, Debug, Clone)]
pub struct ConcurrencyLimit {
    semaphore: Arc<Semaphore>,
}

impl ConcurrencyLimit {
    pub fn as_layer<S>(self) -> impl Fn(S) -> ConcurrencyLimitLayer<S>
    where
        S: Service<Request<Incoming>, Response = Response, Error = Error, Future = ServiceFuture>
            + Send
            + Sync
            + 'static,
    {
        move |inner: S| -> ConcurrencyLimitLayer<S> {
            ConcurrencyLimitLayer::new(Arc::new(inner), self.semaphore.clone())
        }
    }
}

#[derive(Constructor, Debug, Clone)]
pub struct ConcurrencyLimitLayer<S> {
    inner: Arc<S>,
    semaphore: Arc<Semaphore>,
}

impl<S> Service<Request<Incoming>> for ConcurrencyLimitLayer<S>
where
    S: Service<Request<Incoming>, Response = Response, Error = Error, Future = ServiceFuture> + Send + Sync + 'static,
{
    type Error = S::Error;
    type Response = S::Response;
    type Future = S::Future;

    fn call(&self, req: Request<Incoming>) -> Self::Future {
        let inner_clone = self.inner.clone();
        let semaphore_clone = self.semaphore.clone();
        Box::pin(async move {
            if semaphore_clone.available_permits() == 0 {
                return send_503().await;
            };
            let _permit = semaphore_clone
                .acquire()
                .await
                .expect("Semaphore in `ConcurrencyLimit` closed, this should never happen!");
            inner_clone.call(req).await
        })
    }
}

async fn send_503() -> Result<Response> {
    let body_text = "Server busy";
    let body = Full::new(body_text.as_bytes().to_vec().into())
        .map_err(|never| match never {})
        .boxed();
    let response = hyper::Response::builder()
        .status(503)
        .header(hyper::header::CONTENT_LENGTH, body_text.len())
        .header(hyper::header::CONTENT_TYPE, "text/plain")
        .body(body);
    Ok(response?)
}
