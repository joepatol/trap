use std::pin::Pin;
use std::future::Future;
use std::time::Duration;

use derive_more::derive::Constructor;
use http::StatusCode;
use http_body_util::{BodyExt, Full};
use hyper::service::Service;
use tower::layer::Layer;

use crate::error::Error;
use crate::types::*;

#[derive(Constructor)]
pub struct Timeout<S> {
    inner: S,
    timeout: Duration,
}

impl<S> Service<Request> for Timeout<S>
where
    S: Service<Request>,
    <S as Service<Request>>::Future: Send + 'static,
    <S as Service<Request>>::Response: Into<Response>,
    <S as Service<Request>>::Error: Into<Error>,
{
    type Error = Error;
    type Response = Response;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn call(&self, req: Request) -> Self::Future {
        let sleep = tokio::time::sleep(self.timeout);
        let fut = self.inner.call(req);
        let fut = async {
            tokio::select! {
                res = fut => res.map(|r| r.into()).map_err(|e| e.into()),
                _ = sleep => response_408(),
            }
        };
        Box::pin(fut)
    }
}

#[derive(Constructor)]
pub struct TimeoutLayer {
    timeout: Duration,
}

impl<S> Layer<S> for TimeoutLayer {
    type Service = Timeout<S>;

    fn layer(&self, inner: S) -> Self::Service {
        Timeout::new(inner, self.timeout)
    }
}

fn response_408() -> Result<Response, Error> {
    let body_text = "Timeout limit exeeded";
    let body = Full::new(body_text.as_bytes().to_vec().into())
        .map_err(|never| match never {})
        .boxed();
    let response = hyper::Response::builder()
        .status(StatusCode::REQUEST_TIMEOUT)
        .body(body);
    Ok(response?)
}