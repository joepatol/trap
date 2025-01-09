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

use crate::application::{RunningApplication, ApplicationWrapper};
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
        let scope: Scope<A::State> = create_http_scope(&request, &conn, state);
        let mut called_app = ApplicationWrapper::from(&application).call(scope);
        
        stream_request_body(&mut called_app, request.into_body()).await?;
        build_response(called_app).await
    } 
}

async fn stream_request_body<B>(asgi_app: &mut RunningApplication, body: B) -> Result<()>
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
        if more_body == false {
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

async fn build_response(mut asgi_app: RunningApplication) -> Result<Response> {
    let mut builder = hyper::Response::builder();

    let body = match asgi_app.receive_from().await {
        Some(ASGISendEvent::HTTPResponseStart(msg)) => {
            builder = builder.status(msg.status);
            for (bytes_key, bytes_value) in msg.headers.into_iter() {
                builder = builder.header(bytes_key, bytes_value);
            }
            build_body_stream(asgi_app).await
        }
        None => {
            return Err(Error::unexpected_shutdown(
                SRC::Application,
                "stopped without sending HTTP response".into(),
            ))
        }
        Some(msg) => return Err(Error::unexpected_asgi_message(Box::new(msg))),
    };

    Ok(builder.body(body)?)
}

async fn build_body_stream(mut asgi_app: RunningApplication) -> BoxBody<Bytes, Error> {
    let stream = async_stream::stream! {
        let mut more_data = true;
        loop {
            if more_data == false {
                asgi_app.send_to(ASGIReceiveEvent::new_http_disconnect()).await?;
                asgi_app.close().await;
                break
            }
            match asgi_app.receive_from().await {
                Some(ASGISendEvent::HTTPResponseBody(msg)) => {
                    more_data = msg.more_body;
                    yield Ok(msg.body)
                },
                Some(msg) => yield Err(Error::unexpected_asgi_message(Box::new(msg))),
                None => yield Err(Error::unexpected_shutdown(SRC::Application, "stopped while sending HTTP response body".into())),
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
// #[cfg(test)]
// mod tests {
//     use http::StatusCode;
//     use http_body_util::BodyExt;
//     use hyper::Request;
//     use asgispec::prelude::*;

//     use super::serve_http;
//     use crate::application::ApplicationFactory;
//     use crate::error::Error;
//     use crate::types::Response;

//     #[derive(Clone, Debug)]
//     struct MockState;
//     impl State for MockState {}
//     impl std::fmt::Display for MockState {
//         fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//             Ok(())
//         }
//     }

//     #[derive(Clone, Debug)]
//     struct EchoApp {
//         extra_body: Option<String>,
//     }

//     impl EchoApp {
//         pub fn new() -> Self {
//             Self { extra_body: None }
//         }

//         pub fn new_with_body(body: &str) -> Self {
//             Self {
//                 extra_body: Some(body.to_string()),
//             }
//         }
//     }

//     impl ASGIApplication<MockState> for EchoApp {
//         async fn call(&self, _scope: Scope<MockState>, receive: ReceiveFn, send: SendFn) -> Result<(), Box<dyn std::error::Error>> {
//             let mut body = Vec::new();
//             let headers = Vec::from([
//                 ("test".as_bytes().to_vec(), "header".as_bytes().to_vec()),
//                 ("another".as_bytes().to_vec(), "header".as_bytes().to_vec()),
//             ]);
//             loop {
//                 match (receive)().await {
//                     ASGIReceiveEvent::HTTPRequest(msg) => {
//                         body.extend(msg.body.into_iter());
//                         if msg.more_body {
//                             continue;
//                         } else {
//                             let start_msg = ASGISendEvent::new_http_response_start(200, headers.clone());
//                             send(start_msg).await?;
//                             let more_body = self.extra_body.is_some();
//                             let body_msg = ASGISendEvent::new_http_response_body(body.clone(), more_body);
//                             send(body_msg).await?;
//                             if let Some(b) = &self.extra_body {
//                                 let next_msg =
//                                     ASGISendEvent::new_http_response_body(b.to_string().as_bytes().to_vec(), false);
//                                 (send)(next_msg).await?;
//                             };
//                         };
//                     }
//                     ASGIReceiveEvent::HTTPDisconnect(_) => {
//                         break;
//                     }
//                     _ => return Err(Box::new(Error::custom("Invalid message received from server"))),
//                 }
//             }
//             Ok(())
//         }
//     }

//     #[derive(Clone, Debug)]
//     struct ImmediateReturnApp;

//     impl ASGICallable<MockState> for ImmediateReturnApp {
//         async fn call(&self, _scope: Scope<MockState>, _receive: ReceiveFn, _send: SendFn) -> super::Result<()> {
//             Ok(())
//         }
//     }

//     #[derive(Clone, Debug)]
//     struct ErrorOnCallApp;

//     impl ASGICallable<MockState> for ErrorOnCallApp {
//         async fn call(&self, _scope: Scope<MockState>, _receive: ReceiveFn, _send: SendFn) -> super::Result<()> {
//             Err(Error::custom("Immediate error"))
//         }
//     }

//     #[derive(Clone, Debug)]
//     struct ErrorInLoopApp;

//     impl ASGICallable<MockState> for ErrorInLoopApp {
//         async fn call(&self, _scope: Scope<MockState>, receive: ReceiveFn, _send: SendFn) -> super::Result<()> {
//             _ = receive().await;
//             Err(Error::custom("Error in loop"))
//         }
//     }

//     #[derive(Clone, Debug)]
//     struct ErrorInBodyLoopApp;

//     impl ASGICallable<MockState> for ErrorInBodyLoopApp {
//         async fn call(&self, _scope: Scope<MockState>, receive: ReceiveFn, send: SendFn) -> super::Result<()> {
//             _ = receive().await;
//             send(ASGISendEvent::new_http_response_start(200, Vec::new())).await?;
//             Err(Error::custom("Error in loop"))
//         }
//     }

//     #[derive(Clone, Debug)]
//     struct AssertSendErrorApp;

//     impl ASGICallable<MockState> for AssertSendErrorApp {
//         async fn call(&self, _scope: Scope<MockState>, receive: ReceiveFn, send: SendFn) -> super::Result<()> {
//             loop {
//                 match receive().await {
//                     ASGIReceiveEvent::HTTPDisconnect(_) => {
//                         return send(ASGISendEvent::new_shutdown_complete()).await
//                     }
//                     ASGIReceiveEvent::HTTPRequest(_) => {
//                         send(ASGISendEvent::new_http_response_start(200, Vec::new())).await?;
//                         send(ASGISendEvent::new_http_response_body(Vec::new(), false)).await?;
//                         continue
//                     }
//                     _ => return Err(Error::custom("invalid message"))
//                 }
//             }
//         }
//     }

//     #[derive(Clone, Debug)]
//     struct ErrorInDataStreamApp;

//     impl ASGICallable<MockState> for ErrorInDataStreamApp {
//         async fn call(&self, _scope: Scope<MockState>, receive: ReceiveFn, send: SendFn) -> super::Result<()> {
//             let headers = Vec::from([(
//                 String::from("a").as_bytes().to_vec(),
//                 String::from("header").as_bytes().to_vec(),
//             )]);
//             _ = receive().await;
//             let res_start_msg = ASGISendEvent::new_http_response_start(200, headers);
//             send(res_start_msg).await?;
//             let first_body = ASGISendEvent::new_http_response_body(String::from("hello").as_bytes().to_vec(), true);
//             send(first_body).await?;
//             // Instead of more body an invalid message is sent to mimick the error
//             let invalid = ASGISendEvent::new_startup_complete();
//             send(invalid).await?;
//             Ok(())
//         }
//     }

//     async fn response_to_body_string(response: Response) -> String {
//         String::from_utf8(response.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap()
//     }

//     #[tokio::test]
//     async fn test_echo_request_body() {
//         let app = ApplicationFactory::new(EchoApp::new()).build();
//         let request = Request::builder()
//             .body("hello world".to_string())
//             .expect("Failed to build request");
//         let scope = Scope::HTTP(HTTPScope::from_hyper_request(&request, MockState {}));

//         let response = serve_http(app.call(scope).0, request).await.unwrap();
//         assert!(response.status() == StatusCode::OK);
//         let response_body = response_to_body_string(response).await;

//         assert!(response_body == "hello world")
//     }

//     #[tokio::test]
//     async fn test_body_sent_in_parts() {
//         let app = ApplicationFactory::new(EchoApp::new_with_body(" more body")).build();
//         let request = Request::builder()
//             .body("hello world".to_string())
//             .expect("Failed to build request");
//         let scope = Scope::HTTP(HTTPScope::from_hyper_request(&request, MockState {}));

//         let response = serve_http(app.call(scope).0, request).await.unwrap();
//         assert!(response.status() == StatusCode::OK);
//         let response_body = response_to_body_string(response).await;
//         assert!(response_body == "hello world more body")
//     }

//     #[tokio::test]
//     async fn test_app_returns_when_called() {
//         let app = ApplicationFactory::new(ImmediateReturnApp {}).build();
//         let request = Request::builder()
//             .body("hello world".to_string())
//             .expect("Failed to build request");
//         let scope = Scope::HTTP(HTTPScope::from_hyper_request(&request, MockState {}));
//         let response = serve_http(app.call(scope).0, request).await;

//         assert!(response.is_err_and(
//             |e| e.to_string() == "Application shutdown unexpectedly. stopped without sending HTTP response"
//         ));
//     }

//     #[tokio::test]
//     async fn test_app_fails_when_called() {
//         let app = ApplicationFactory::new(ErrorOnCallApp {}).build();
//         let request = Request::builder()
//             .body("hello world".to_string())
//             .expect("Failed to build request");
//         let scope = Scope::HTTP(HTTPScope::from_hyper_request(&request, MockState {}));
//         let response = serve_http(app.call(scope).0, request).await;

//         assert!(response.is_err_and(
//             |e| e.to_string() == "Application shutdown unexpectedly. stopped without sending HTTP response"
//         ));
//     }

//     #[tokio::test]
//     async fn test_app_raises_error_while_communicating() {
//         let app = ApplicationFactory::new(ErrorInLoopApp {}).build();
//         let request = Request::builder()
//             .body("hello world".to_string())
//             .expect("Failed to build request");
//         let scope = Scope::HTTP(HTTPScope::from_hyper_request(&request, MockState {}));
//         let response = serve_http(app.call(scope).0, request).await;

//         assert!(response.is_err_and(
//             |e| e.to_string() == "Application shutdown unexpectedly. stopped without sending HTTP response"
//         ));
//     }

//     #[tokio::test]
//     async fn test_app_raises_error_while_sending_body() {
//         let app = ApplicationFactory::new(ErrorInBodyLoopApp {}).build();
//         let request = Request::builder()
//             .body("hello world".to_string())
//             .expect("Failed to build request");
//         let scope = Scope::HTTP(HTTPScope::from_hyper_request(&request, MockState {}));
//         let response = serve_http(app.call(scope).0, request).await;

//         let body = response.unwrap().into_body().collect().await;
//         assert!(body.is_err_and(|e| e.to_string() == "Application shutdown unexpectedly. stopped while sending HTTP response body"))

//     }


//     #[tokio::test]
//     async fn test_error_while_streaming_body() {
//         let app = ApplicationFactory::new(ErrorInDataStreamApp {}).build();
//         let request = Request::builder()
//             .body("hello world".to_string())
//             .expect("Failed to build request");
//         let scope = Scope::HTTP(HTTPScope::from_hyper_request(&request, MockState {}));

//         let response = serve_http(app.call(scope).0, request).await.unwrap();
//         let body = response.into_body().collect().await;

//         assert!(body.is_err_and(|e| e.to_string() == "Unexpected ASGI message received. StartupComplete(LifespanStartupComplete { type_: \"lifespan.startup.complete\" })"));
//     }

//     #[tokio::test]
//     async fn test_headers_ok() {
//         let app = ApplicationFactory::new(EchoApp::new()).build();
//         let request = Request::builder()
//             .body("hello world".to_string())
//             .expect("Failed to build request");
//         let scope = Scope::HTTP(HTTPScope::from_hyper_request(&request, MockState {}));

//         let response = serve_http(app.call(scope).0, request).await.unwrap();
//         assert!(response.status() == StatusCode::OK);
//         let headers = response.headers();

//         assert!(headers.get("test").and_then(|v| Some(v.to_str().unwrap())) == Some("header"));
//         assert!(headers.get("another").and_then(|v| Some(v.to_str().unwrap())) == Some("header"));
//     }

//     #[tokio::test]
//     async fn test_send_is_error_after_disconnect() {
//         let request = Request::builder()
//             .body(String::default())
//             .expect("Failed to build request");
//         let scope = Scope::HTTP(HTTPScope::from_hyper_request(&request, MockState {}));
//         let (app, rx) = ApplicationFactory::new(AssertSendErrorApp {}).build().call(scope);
        
//         let response = serve_http(app.clone(), request).await;
//         assert!(response.is_ok());
//         let body = response_to_body_string(response.unwrap()).await;
//         assert!(body == "");

//         let app_out = rx.await.unwrap();
//         assert!(app_out.is_err_and(|e| e.to_string() == "Disconnected client"))
//     }
// }
