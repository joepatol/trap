use std::fmt::Debug;

use bytes::{Buf, Bytes};
use derive_more::derive::Constructor;
use futures::StreamExt;
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, StreamBody};
use hyper::body::{Body, Frame};
use hyper::Request;
use asgispec::prelude::*;
use asgispec::scope::HTTPScope;

use crate::application::{CalledApplication, ApplicationWrapper};
use crate::error::{Error, Result, UnexpectedShutdownSrc as SRC};
use crate::types::{ConnectionInfo, Response};

#[derive(Constructor)]
pub(crate) struct HTTPHandler;

impl HTTPHandler {    
    pub async fn serve<A, B>(self, application: A, request: Request<B>, conn: ConnectionInfo, state: A::State) -> Result<Response>
    where
        A: ASGIApplication + 'static,
        B: Body + Send + 'static,
        <B as hyper::body::Body>::Error: Debug,
    {
        let scope = create_http_scope(&request, &conn, state);
        let mut called_app = ApplicationWrapper::from(&application).call(scope);
        
        stream_request_body(&mut called_app, request.into_body()).await?;
        build_response(called_app).await
    } 
}

async fn stream_request_body<B>(asgi_app: &mut CalledApplication, body: B) -> Result<()>
where
    B: Body + Send + 'static,
    <B as hyper::body::Body>::Error: Debug,
{
    // This implementation will always send an additional ASGI message with an
    // empty body once the stream is finished.
    let mut stream = body.into_data_stream().boxed();
    let mut part;
    let mut more_body = true;

    loop {
        if !more_body {
            break;
        }

        part = stream.next().await;

        let data = part.map_or_else(
            || {
                more_body = false;
                Ok(Vec::new())
            },
            |part_result| {
                part_result
                    .map(|mut data| data.copy_to_bytes(data.remaining()).to_vec())
                    .map_err(|e| Error::custom(format!("Failed to read body: {e:?}")))
            },
        )?;

        let msg = ASGIReceiveEvent::new_http_request(data, more_body);
        asgi_app.send_to(msg).await?;
    }
    Ok(())
}

async fn build_response(mut asgi_app: CalledApplication) -> Result<Response> {
    let mut builder = hyper::Response::builder();

    let body = match asgi_app.receive_from().await {
        Ok(ASGISendEvent::HTTPResponseStart(msg)) => {
            builder = builder.status(msg.status);
            for (bytes_key, bytes_value) in msg.headers.into_iter() {
                builder = builder.header(bytes_key, bytes_value);
            }
            build_body_stream(asgi_app).await
        }
        Ok(msg) => return Err(Error::unexpected_asgi_message(Box::new(msg))),
        Err(e) => {
            return Err(Error::unexpected_shutdown(
                SRC::Application,
                format!("Stopped without sending HTTP response: {e}").into(),
            ))
        }
    };

    Ok(builder.body(body)?)
}

async fn build_body_stream(mut asgi_app: CalledApplication) -> BoxBody<Bytes, Error> {
    let stream = async_stream::stream! {
        let mut more_data = true;
        loop {
            if !more_data {
                // If sending the disconnect event fails, it's because the application
                // cannot receive any more messages. We don't care...
                _ = asgi_app.send_to(ASGIReceiveEvent::new_http_disconnect()).await;
                asgi_app.close();
                break
            }
            match asgi_app.receive_from().await {
                Ok(ASGISendEvent::HTTPResponseBody(msg)) => {
                    more_data = msg.more_body;
                    yield Ok(msg.body)
                },
                Ok(msg) => yield Err(Error::unexpected_asgi_message(Box::new(msg))),
                Err(e) => yield Err(Error::unexpected_shutdown(SRC::Application, format!("{e}").into())),
            }
        }
    };

    let byte_frame_stream = stream.map(|item| match item {
        Ok(data) => Ok(Frame::data(Bytes::from(data))),
        Err(e) => Err(e),
    });

    BoxBody::new(StreamBody::new(byte_frame_stream))
}

fn create_http_scope<B: Body, S: State>(request: &Request<B>, connection_info: &ConnectionInfo, state: S) -> Scope<S> {
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

    use crate::types::Response;
    use crate::types::ConnectionInfo;
    use crate::applications::*;

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
        let handler = HTTPHandler::new();
        let request = Request::builder()
            .body("hello world".to_string())
            .expect("Failed to build request");

        let response = handler.serve(
            EchoApp::new(),
            request, 
            build_conn_info(),
            MockState {}
        ).await.unwrap();

        assert!(response.status() == StatusCode::OK);
        assert!(response_to_body_string(response).await == "hello world")
    }

    #[tokio::test]
    async fn test_body_sent_in_parts() {
        let handler = HTTPHandler::new();
        let request = Request::builder()
            .body("hello world".to_string())
            .expect("Failed to build request");

        let response = handler.serve(
            EchoApp::new_with_body("more body"),
            request, 
            build_conn_info(),
            MockState {}
        ).await.unwrap();

        assert!(response.status() == StatusCode::OK);
        assert!(response_to_body_string(response).await == "hello worldmore body")
    }

    #[tokio::test]
    async fn test_app_returns_when_called() {
        let handler = HTTPHandler::new();
        let request = Request::builder()
            .body("hello world".to_string())
            .expect("Failed to build request");

        let response = handler.serve(
            ImmediateReturnApp {},
            request, 
            build_conn_info(),
            MockState {}
        ).await;

        assert!(response.is_err_and(
            |e| {
                e.to_string() == "Application shutdown unexpectedly. Stopped without sending HTTP response: receiving from an empty and closed channel"
            }
        ));
    }

    #[tokio::test]
    async fn test_app_fails_when_called() {
        let handler = HTTPHandler::new();
        let request = Request::builder()
            .body("hello world".to_string())
            .expect("Failed to build request");

        let response = handler.serve(
            ErrorOnCallApp {},
            request, 
            build_conn_info(),
            MockState {}
        ).await;

        assert!(response.is_err_and(|e| {
            e.to_string() == "Application shutdown unexpectedly. Stopped without sending HTTP response: receiving from an empty and closed channel"
        }));
    }

    #[tokio::test]
    async fn test_app_raises_error_while_communicating() {
        let handler = HTTPHandler::new();
        let request = Request::builder()
            .body("hello world".to_string())
            .expect("Failed to build request");

        let response = handler.serve(
            ErrorInLoopApp {},
            request, 
            build_conn_info(),
            MockState {}
        ).await;

        assert!(response.is_err_and(
            |e| e.to_string() == "Application shutdown unexpectedly. Stopped without sending HTTP response: receiving from an empty and closed channel"
        ));
    }

    #[tokio::test]
    async fn test_app_raises_error_while_sending_body() {
        let handler = HTTPHandler::new();
        let request = Request::builder()
            .body("hello world".to_string())
            .expect("Failed to build request");

        let response = handler.serve(
            ErrorInBodyLoopApp {},
            request, 
            build_conn_info(),
            MockState {}
        ).await;

        let body = response.unwrap().into_body().collect().await;
        assert!(body.is_err_and(|e| {
            e.to_string() == "Application shutdown unexpectedly. Application error: TestError(\"Error in loop\")"
        }))

    }

    #[tokio::test]
    async fn test_error_while_streaming_body() {
        let handler = HTTPHandler::new();
        let request = Request::builder()
            .body("hello world".to_string())
            .expect("Failed to build request");

        let response = handler.serve(
            ErrorInDataStreamApp {},
            request, 
            build_conn_info(),
            MockState {}
        ).await;

        let body = response.unwrap().into_body().collect().await;
        println!("{:?}", body.as_ref().map_err(|e| e.to_string()));
        assert!(body.is_err_and(|e| e.to_string() == "Unexpected ASGI message received. StartupComplete(LifespanStartupCompleteEvent)"))
    }

    #[tokio::test]
    async fn test_headers_ok() {
        let handler = HTTPHandler::new();
        let request = Request::builder()
            .body("hello world".to_string())
            .expect("Failed to build request");

        let response = handler.serve(
            EchoApp::new(),
            request, 
            build_conn_info(),
            MockState {}
        ).await.unwrap();

        assert!(response.status() == StatusCode::OK);
        let headers = response.headers();
        assert!(headers.get("test").and_then(|v| Some(v.to_str().unwrap())) == Some("header"));
        assert!(headers.get("another").and_then(|v| Some(v.to_str().unwrap())) == Some("header"));
    }
}
