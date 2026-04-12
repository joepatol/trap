use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use http::Request;
use http_body::Body;
use http_body_util::combinators::UnsyncBoxBody;
use http_body_util::{BodyExt, Full};
use tower::{BoxError, Layer, Service};

/// Tower layer that intercepts all errors from the inner middleware stack and converts them
/// into appropriate HTTP responses.
///
/// Error mapping:
/// - [`tower::load_shed::error::Overloaded`] → `503 Service Unavailable`
/// - [`tower::timeout::error::Elapsed`]      → `504 Gateway Timeout`
/// - All other errors                        → `500 Internal Server Error`
#[derive(Clone, Default)]
pub(crate) struct ErrorHandlerLayer;

impl ErrorHandlerLayer {
    pub fn new() -> Self {
        Self
    }
}

impl<S> Layer<S> for ErrorHandlerLayer {
    type Service = ErrorHandlerService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        ErrorHandlerService { inner }
    }
}

pub(crate) struct ErrorHandlerService<S> {
    inner: S,
}

impl<S: Clone> Clone for ErrorHandlerService<S> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<S, B, ResBody> Service<Request<B>> for ErrorHandlerService<S>
where
    S: Service<Request<B>, Response = http::Response<ResBody>, Error = BoxError>,
    S::Future: Send + 'static,
    ResBody: Body<Data = Bytes> + Send + 'static,
    ResBody::Error: Into<BoxError>,
{
    type Response = http::Response<UnsyncBoxBody<Bytes, BoxError>>;
    type Error = BoxError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, BoxError>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request<B>) -> Self::Future {
        let fut = self.inner.call(request);
        Box::pin(async move {
            match fut.await {
                Ok(response) => Ok(response.map(|body| body.map_err(Into::into).boxed_unsync())),
                Err(e) => Ok(error_response(&e)),
            }
        })
    }
}

fn error_response(e: &BoxError) -> http::Response<UnsyncBoxBody<Bytes, BoxError>> {
    if e.is::<tower::load_shed::error::Overloaded>() {
        plain_response(503, "Service Unavailable")
    } else if e.is::<tower::timeout::error::Elapsed>() {
        plain_response(504, "Gateway Timeout")
    } else {
        plain_response(500, "Internal Server Error")
    }
}

fn plain_response(status: u16, body_text: &'static str) -> http::Response<UnsyncBoxBody<Bytes, BoxError>> {
    let body = Full::new(Bytes::from_static(body_text.as_bytes()))
        .map_err(|never| match never {})
        .boxed_unsync();
    http::Response::builder()
        .status(status)
        .header(http::header::CONTENT_LENGTH, body_text.len())
        .header(http::header::CONTENT_TYPE, "text/plain")
        .body(body)
        .expect("error response construction is infallible")
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use http_body_util::{Empty, Full, BodyExt};
    use tower::{BoxError, Service, service_fn};
    use http::{Request, Response};

    use super::ErrorHandlerService;

    #[tokio::test]
    async fn test_overloaded_is_503() {
        async fn svc(_: Request<()>) -> Result<Response<Empty<Bytes>>, BoxError> {
            Err(Box::new(tower::load_shed::error::Overloaded::new()))
        }

        let mut service = ErrorHandlerService { inner: service_fn(svc) };

        let response = service.call(Request::<()>::new(())).await.unwrap();
        assert_eq!(response.status(), 503);
    }

    #[tokio::test]
    async fn test_timeout_is_504() {
        async fn svc(_: Request<()>) -> Result<Response<Empty<Bytes>>, BoxError> {
            Err(Box::new(tower::timeout::error::Elapsed::new()))
        }

        let mut service = ErrorHandlerService { inner: service_fn(svc) };

        let response = service.call(Request::<()>::new(())).await.unwrap();
        assert_eq!(response.status(), 504);
    }

    #[tokio::test]
    async fn test_other_error_is_500() {
        async fn svc(_: Request<()>) -> Result<Response<Empty<Bytes>>, BoxError> {
            Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, "other error")))
        }

        let mut service = ErrorHandlerService { inner: service_fn(svc) };

        let response = service.call(Request::<()>::new(())).await.unwrap();
        assert_eq!(response.status(), 500);
    }

    #[tokio::test]
    async fn test_success_response_passes_through() {
        async fn svc(_: Request<()>) -> Result<Response<Full<Bytes>>, BoxError> {
            Ok(Response::new(Full::new(Bytes::from_static(b"success"))))
        }

        let mut service = ErrorHandlerService { inner: service_fn(svc) };

        let response = service.call(Request::<()>::new(())).await.unwrap();
        assert_eq!(response.status(), 200);
        let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(body_bytes, Bytes::from_static(b"success"));
    }
}