use std::future::Future;
use std::pin::Pin;

use derive_more::derive::Constructor;
use http::StatusCode;
use tower::layer::Layer;
use http::Request;
use http_body::Body;
use tower::Service;
use http_body_util::{Full, BodyExt, Limited};

use crate::error::Error;
use crate::types::Response;

#[derive(Constructor, Clone)]
pub struct RequestBodyLimit<S> {
    inner: S,
    limit: usize,
}

impl<S, ReqBody> Service<Request<ReqBody>> for RequestBodyLimit<S> 
where
    ReqBody: Body,
    S: Service<Request<Limited<ReqBody>>, Error = Error>,
    <S as Service<Request<Limited<ReqBody>>>>::Future: Send + 'static,
    <S as Service<Request<Limited<ReqBody>>>>::Response: Into<Response>,
    <S as Service<Request<Limited<ReqBody>>>>::Error: Into<Error>,
{
    type Error = Error;
    type Response = Response;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<ReqBody>) -> Self::Future {
        let content_length = req
            .headers()
            .get(http::header::CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok()?.parse::<usize>().ok());

        let body_limit = match content_length {
            Some(len) if len > self.limit => return Box::pin(async {response_413()}),
            Some(len) => self.limit.min(len),
            None => self.limit,
        };

        let req = req.map(|body| Limited::new(body, body_limit));

        let fut = self.inner.call(req);
        let fut = async {
            fut.await.and_then(|value| Ok(value.into())).map_err(|e| e.into())
        };
        Box::pin(fut)
    }
}

#[derive(Constructor)]
pub struct RequestBodyLimitLayer {
    limit: usize
}

impl<S> Layer<S> for RequestBodyLimitLayer {
    type Service = RequestBodyLimit<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RequestBodyLimit::new(inner, self.limit)
    }
}

fn response_413() -> Result<Response, Error> {
    let body_text = "Payload limit exceeded";
    let body = Full::new(body_text.as_bytes().to_vec().into())
        .map_err(|never| match never {})
        .boxed();
    let response = hyper::Response::builder()
        .status(StatusCode::PAYLOAD_TOO_LARGE)
        .body(body);
    Ok(response?)
}