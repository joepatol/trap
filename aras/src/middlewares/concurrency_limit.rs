use std::future::Future;
use std::sync::Arc;
use std::pin::Pin;

use derive_more::derive::Constructor;
use http::StatusCode;
use tokio::sync::Semaphore;
use tower::layer::Layer;
use http::Request;
use http_body::Body;
use hyper::service::Service;
use http_body_util::{Full, BodyExt};

use crate::error::Error;
use crate::types::Response;

#[derive(Constructor)]
pub struct ConcurrencyLimit<S> {
    inner: S,
    limit: Arc<Semaphore>,
}

impl<S, ReqBody> Service<Request<ReqBody>> for ConcurrencyLimit<S> 
where
    ReqBody: Body,
    S: Service<Request<ReqBody>>,
    <S as Service<Request<ReqBody>>>::Future: Send + 'static,
    <S as Service<Request<ReqBody>>>::Response: Into<Response>,
    <S as Service<Request<ReqBody>>>::Error: Into<Error>,
{
    type Error = Error;
    type Response = Response;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn call(&self, req: Request<ReqBody>) -> Self::Future {
        let semaphore = self.limit.clone();
        let fut = self.inner.call(req);
        Box::pin(async move {
            if semaphore.available_permits() == 0 {
                return response_503();
            };
            let _permit = semaphore
                .acquire()
                .await
                .expect("Semaphore in `ConcurrencyLimit` closed, this should never happen!");
            fut.await.and_then(|value| Ok(value.into())).map_err(|e| e.into())
        })
    }
}

#[derive(Constructor)]
pub struct ConcurrencyLimitLayer {
    limit: usize
}

impl<S> Layer<S> for ConcurrencyLimitLayer {
    type Service = ConcurrencyLimit<S>;

    fn layer(&self, inner: S) -> Self::Service {
        ConcurrencyLimit::new(inner, Arc::new(Semaphore::new(self.limit)))
    }
}

fn response_503() -> Result<Response, Error> {
    let body_text = "Server busy";
    let body = Full::new(body_text.as_bytes().to_vec().into())
        .map_err(|never| match never {})
        .boxed();
    let response = hyper::Response::builder()
        .status(StatusCode::SERVICE_UNAVAILABLE)
        .body(body);
    Ok(response?)
}