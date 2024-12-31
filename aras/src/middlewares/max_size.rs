use std::fmt::Debug;
use std::sync::Arc;

use derive_more::derive::Constructor;
use http_body_util::{BodyExt, Full};
use hyper::body::{Body, Incoming};
use hyper::service::Service;
use hyper::Request;

use crate::error::{Error, Result};
use crate::types::{Response, ServiceFuture};

#[derive(Constructor, Debug, Clone)]
pub struct ContentLengthLimit {
    max_size: u64,
}

impl ContentLengthLimit {
    pub fn as_layer<S>(self) -> impl Fn(S) -> ContentLengthLimitLayer<S>
    where
        S: Service<Request<Incoming>, Response = Response, Error = Error, Future = ServiceFuture>
            + Send
            + Sync
            + 'static,
    {
        move |inner: S| -> ContentLengthLimitLayer<S> { 
            ContentLengthLimitLayer::new(Arc::new(inner), self.max_size) 
        }
    }
}

#[derive(Constructor, Debug, Clone)]
pub struct ContentLengthLimitLayer<S> {
    inner: Arc<S>,
    max_size: u64,
}

impl<S> Service<Request<Incoming>> for ContentLengthLimitLayer<S>
where
    S: Service<Request<Incoming>, Response = Response, Error = Error, Future = ServiceFuture> + Send + Sync + 'static,
{
    type Error = S::Error;
    type Response = S::Response;
    type Future = S::Future;

    fn call(&self, req: Request<Incoming>) -> Self::Future {
        let inner_clone = self.inner.clone();

        // For chunked data, the check is skipped
        if let Some(h) = req.headers().get(hyper::header::TRANSFER_ENCODING) {
            if h == "chunked" {
                return Box::pin(async move { inner_clone.call(req).await })
            }
        }

        let content_length = match req.body().size_hint().upper() {
            Some(v) => v,
            None => self.max_size + 1,  // If no content-length provided always refuse
        };

        let too_large = content_length > self.max_size;

        Box::pin(async move {
            if too_large {
                return send_413().await;
            };
            inner_clone.call(req).await
        })
    }
}

async fn send_413() -> Result<Response> {
    let body_text = "Payload too large, or 'Content-length' not provided";
    let body = Full::new(body_text.as_bytes().to_vec().into())
        .map_err(|never| match never {})
        .boxed();
    let response = hyper::Response::builder()
        .status(413)
        .header(hyper::header::CONTENT_LENGTH, body_text.len())
        .header(hyper::header::CONTENT_TYPE, "text/plain")
        .body(body);
    Ok(response?)
}
