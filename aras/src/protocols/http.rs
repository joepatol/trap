use std::fmt::Display;
use std::sync::Arc;
use std::time::Duration;

use asgispec::prelude::*;
use asgispec::scope::HTTPScope;
use bytes::{Buf, Bytes};
use derive_more::derive::Constructor;
use futures::StreamExt;
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, StreamBody};
use hyper::body::{Body, Frame};
use hyper::Request;

use crate::communication::{CommunicatorFactory, ReceiveFromApp, SendToApp};
use crate::errors::{Error, Result};
use crate::types::{ConnectionInfo, Response};

#[derive(Constructor)]
pub(crate) struct HTTPHandler {
    timeout_secs: u64,
}

impl HTTPHandler {
    pub async fn serve<A, B>(
        self,
        application: A,
        request: Request<B>,
        conn: ConnectionInfo,
        state: A::State,
    ) -> Result<Response>
    where
        A: ASGIApplication + 'static,
        B: Body + Send + 'static,
        B::Error: Display,
    {
        let factory = CommunicatorFactory::new();
        let scope = create_http_scope(&request, &conn, state);
        
        let (mut send_to_app, receive_from_app) = factory.build(application, scope);

        let response = tokio::try_join!(
            self.read_body(&mut send_to_app, request.into_body()),
            self.make_response(receive_from_app),
        );

        // If sending the disconnect event fails, it's because the application
        // cannot receive any more messages. We don't care...
        _ = send_to_app.push(ASGIReceiveEvent::new_http_disconnect()).await;

        Ok(response?.1)
    }

    async fn read_body<B>(&self, send_to_app: &mut SendToApp, body: B) -> Result<()>
    where
        B: Body + Send + 'static,
        B::Error: Display,
    {
        let timeout = Duration::from_secs(self.timeout_secs);
        let mut more_body = true;
        let mut stream = body.into_data_stream().boxed();

        while let Some(part) = tokio::time::timeout(timeout, stream.next()).await? {
            let mut data = part.map_err(|e| Error::custom(format!("Failed to read body: {e}")))?;

            let size = data.remaining();
            let bytes = data.copy_to_bytes(size);
            more_body = stream.size_hint().1.map_or(true, |u| u > 0);

            let msg = ASGIReceiveEvent::new_http_request(bytes, more_body);
            send_to_app.push(msg).await?;
        }

        if more_body {
            // The stream terminated but the size hint did not indicate the end of the body.
            // Send an empty body with more_body = false to indicate the end of the body.
            let msg = ASGIReceiveEvent::new_http_request(Bytes::new(), false);
            send_to_app.push(msg).await?;
        }

        Ok(())
    }

    async fn make_response(&self, mut receive_from_app: ReceiveFromApp) -> Result<Response> {
        let timeout = Duration::from_secs(self.timeout_secs);

        let response_start_event = match tokio::time::timeout(timeout, receive_from_app.next()).await? {
            Ok(ASGISendEvent::HTTPResponseStart(msg)) => msg,
            Ok(msg) => return Err(Error::unexpected_asgi_message(Arc::new(msg))),
            Err(e) => return Err(e),
        };

        let body_stream = async_stream::stream! {
            let mut more_data = true;
            loop {
                if !more_data {
                    break
                }
                match tokio::time::timeout(timeout, receive_from_app.next()).await? {
                    Ok(ASGISendEvent::HTTPResponseBody(msg)) => {
                        more_data = msg.more_body;
                        yield Ok(Frame::data(msg.body))
                    },
                    Ok(msg) => yield Err(Error::unexpected_asgi_message(Arc::new(msg))),
                    Err(e) => yield Err(e),
                }
            }
        };

        let mut builder = hyper::Response::builder();
        builder = builder.status(response_start_event.status);
        for (bytes_key, bytes_value) in response_start_event.headers.into_iter() {
            builder = builder.header(bytes_key, bytes_value);
        }
        let body = BoxBody::new(StreamBody::new(body_stream));
        Ok(builder.body(body)?)
    }
}

fn create_http_scope<B, S: State>(request: &Request<B>, connection_info: &ConnectionInfo, state: S) -> Scope<S> {
    let scope = HTTPScope::new(
        ASGIScope::default(),
        format!("{:?}", request.version()),
        request.method().as_str().to_owned(),
        String::from("http"),
        request.uri().path().to_owned(),
        request.uri().to_string().as_bytes().to_vec(),
        request.uri().query().unwrap_or("").as_bytes().to_vec(),
        String::from(""), // Optional, default for now
        request
            .headers()
            .into_iter()
            .map(|(name, value)| (name.as_str().as_bytes().to_vec(), value.as_bytes().to_vec()))
            .collect(),
        Some((connection_info.client_ip.to_owned(), connection_info.client_port)),
        Some((connection_info.server_ip.to_owned(), connection_info.server_port)),
        Some(state),
    );
    scope.into()
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddrV4;

    use http::StatusCode;
    use http_body_util::BodyExt;
    use hyper::Request;

    use crate::applications::*;
    use crate::types::ConnectionInfo;
    use crate::types::Response;

    use super::HTTPHandler;

    pub fn build_conn_info() -> ConnectionInfo {
        ConnectionInfo::new(
            std::net::SocketAddr::V4(SocketAddrV4::new([127, 0, 0, 1].into(), 2)),
            std::net::SocketAddr::V4(SocketAddrV4::new([0, 0, 0, 0].into(), 80)),
        )
    }

    async fn response_to_body_string(response: Response) -> String {
        String::from_utf8(response.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap()
    }

    #[tokio::test]
    async fn test_echo_request_body() {
        let handler = HTTPHandler::new(10);
        let request = Request::builder()
            .body("hello world".to_string())
            .expect("Failed to build request");

        let response = handler
            .serve(EchoApp::new(), request, build_conn_info(), MockState {})
            .await
            .unwrap();

        assert!(response.status() == StatusCode::OK);
        assert!(response_to_body_string(response).await == "hello world")
    }

    #[tokio::test]
    async fn test_body_sent_in_parts() {
        let handler = HTTPHandler::new(10);
        let request = Request::builder()
            .body("hello world".to_string())
            .expect("Failed to build request");

        let response = handler
            .serve(
                EchoApp::new_with_body("more body"),
                request,
                build_conn_info(),
                MockState {},
            )
            .await
            .unwrap();

        assert!(response.status() == StatusCode::OK);
        assert!(response_to_body_string(response).await == "hello worldmore body")
    }

    #[tokio::test]
    async fn test_app_returns_when_called() {
        let handler = HTTPHandler::new(10);
        let request = Request::builder()
            .body("hello world".to_string())
            .expect("Failed to build request");

        let response = handler
            .serve(ImmediateReturnApp {}, request, build_conn_info(), MockState {})
            .await;

        assert!(response.is_err_and(|e| { e.to_string() == "Application is not running" }));
    }

    #[tokio::test]
    async fn test_app_fails_when_called() {
        let handler = HTTPHandler::new(10);
        let request = Request::builder()
            .body("hello world".to_string())
            .expect("Failed to build request");

        let response = handler
            .serve(ErrorOnCallApp {}, request, build_conn_info(), MockState {})
            .await;

        assert!(response.is_err_and(|e| {
            e.to_string() == "Application error: TestError(\"Immediate error\")"
        }));
    }

    #[tokio::test]
    async fn test_app_raises_error_while_communicating() {
        let handler = HTTPHandler::new(10);
        let request = Request::builder()
            .body("hello world".to_string())
            .expect("Failed to build request");

        let response = handler
            .serve(ErrorInLoopApp {}, request, build_conn_info(), MockState {})
            .await;

        assert!(response.is_err_and(
            |e| {
                e.to_string() == "Application error: TestError(\"Error in loop\")"
            }
        ));
    }

    #[tokio::test]
    async fn test_app_raises_error_while_sending_body() {
        let handler = HTTPHandler::new(10);
        let request = Request::builder()
            .body("hello world".to_string())
            .expect("Failed to build request");

        let response = handler
            .serve(ErrorInBodyLoopApp {}, request, build_conn_info(), MockState {})
            .await;

        assert!(response.is_err_and(|e| {
            e.to_string() == "Application error: TestError(\"Error in loop\")"
        }))
    }

    #[tokio::test]
    async fn test_error_while_streaming_body() {
        let handler = HTTPHandler::new(10);
        let request = Request::builder()
            .body("hello world".to_string())
            .expect("Failed to build request");

        let response = handler
            .serve(ErrorInDataStreamApp {}, request, build_conn_info(), MockState {})
            .await;

        let body = response.unwrap().into_body().collect().await;

        assert!(body.is_err_and(
            |e| {
                e.to_string() == "Unexpected ASGI message received. StartupComplete(LifespanStartupCompleteEvent)"
            }
        ))
    }

    #[tokio::test]
    async fn test_headers_ok() {
        let handler = HTTPHandler::new(10);
        let request = Request::builder()
            .body("hello world".to_string())
            .expect("Failed to build request");

        let response = handler
            .serve(EchoApp::new(), request, build_conn_info(), MockState {})
            .await
            .unwrap();

        assert!(response.status() == StatusCode::OK);
        let headers = response.headers();
        assert!(headers.get("test").and_then(|v| Some(v.to_str().unwrap())) == Some("header"));
        assert!(headers.get("another").and_then(|v| Some(v.to_str().unwrap())) == Some("header"));
    }

    #[tokio::test]
    async fn test_backpressure_timeout() {
        let handler = HTTPHandler::new(1);
        let request = Request::builder()
            .body("hello world".to_string())
            .expect("Failed to build request");

        let response = handler
            .serve(WaitApp {}, request, build_conn_info(), MockState {})
            .await;

        assert!(response.is_err_and(|e| {
            e.to_string() == "ASGI await timeout elapsed" 
        }));
    }
}
