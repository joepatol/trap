use std::fmt::Display;
use std::time::Duration;

use asgispec::prelude::*;
use bytes::{Buf, Bytes};
use derive_more::derive::Constructor;
use futures::StreamExt;
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, StreamBody};
use hyper::body::{Body, Frame};
use hyper::Request;

use crate::communication::{ReceiveFromASGIApp, SendToASGIApp};
use crate::types::Response;
use crate::{ArasError, ArasResult};

#[derive(Constructor)]
pub(crate) struct HTTPHandler {
    timeout: Duration,
}

impl HTTPHandler {
    pub async fn handle<B>(
        self,
        mut send_to_app: impl SendToASGIApp,
        receive_from_app: impl ReceiveFromASGIApp + 'static,
        request: Request<B>,
    ) -> ArasResult<Response>
    where
        B: Body + Send + 'static,
        B::Error: Display,
    {
        let response = tokio::try_join!(
            self.read_body(&mut send_to_app, request.into_body()),
            self.make_response(receive_from_app),
        );

        // If sending the disconnect event fails, it's because the application
        // cannot receive any more messages. We don't care...
        _ = send_to_app
            .send(ASGIReceiveEvent::new_http_disconnect())
            .await;

        Ok(response?.1)
    }

    async fn read_body<B>(&self, send_to_app: &mut impl SendToASGIApp, body: B) -> ArasResult<()>
    where
        B: Body + Send + 'static,
        B::Error: Display,
    {
        let mut more_body = true;
        let mut stream = body.into_data_stream().boxed();

        while let Some(part) = tokio::time::timeout(self.timeout, stream.next()).await? {
            let mut data =
                part.map_err(|e| ArasError::custom(format!("Failed to read body: {e}")))?;

            let size = data.remaining();
            let bytes = data.copy_to_bytes(size);
            more_body = stream.size_hint().1.is_none_or(|u| u > 0);

            let msg = ASGIReceiveEvent::new_http_request(bytes, more_body);
            send_to_app.send(msg).await?;
        }

        if more_body {
            // The stream terminated but the size hint did not indicate the end of the body.
            // Send an empty body with more_body = false to indicate the end of the body.
            let msg = ASGIReceiveEvent::new_http_request(Bytes::new(), false);
            send_to_app.send(msg).await?;
        }

        Ok(())
    }

    async fn make_response(
        &self,
        mut receive_from_app: impl ReceiveFromASGIApp + 'static,
    ) -> ArasResult<Response> {
        let response_start_event =
            match tokio::time::timeout(self.timeout, receive_from_app.receive()).await? {
                Ok(ASGISendEvent::HTTPResponseStart(msg)) => msg,
                Ok(msg) => return Err(ArasError::unexpected_asgi_message(&format!("{msg:?}"))),
                Err(e) => return Err(e.into()),
            };

        let timeout_for_stream = self.timeout;
        let body_stream = async_stream::stream! {
            let mut more_data = true;
            loop {
                if !more_data {
                    break
                }
                match tokio::time::timeout(timeout_for_stream, receive_from_app.receive()).await? {
                    Ok(ASGISendEvent::HTTPResponseBody(msg)) => {
                        more_data = msg.more_body;
                        yield Ok(Frame::data(msg.body))
                    },
                    Ok(msg) => yield Err(ArasError::unexpected_asgi_message(&format!("{msg:?}"))),
                    Err(e) => yield Err(e.into()),
                };
            }
        };

        let mut builder = hyper::Response::builder();
        builder = builder.status(response_start_event.status);
        for (bytes_key, bytes_value) in response_start_event.headers.into_iter() {
            builder = builder.header(bytes_key.to_vec(), bytes_value.to_vec());
        }
        let body = BoxBody::new(StreamBody::new(body_stream));
        Ok(builder.body(body)?)
    }
}

#[cfg(test)]
mod tests {
    use asgispec::prelude::*;
    use bytes::Bytes;
    use core::panic;
    use http::Request;
    use http_body_util::BodyExt;
    use std::time::Duration;

    use super::HTTPHandler;
    use crate::mocks::communication::{
        DeterministicReceiveFromApp, SendToAppCollector, SendToAppFail,
    };
    use crate::types::Response;
    use crate::ArasError;

    fn build_request(body: &str) -> Request<String> {
        Request::builder()
            .body(body.to_string())
            .expect("Failed to build request")
    }

    async fn response_to_body_string(response: Response) -> String {
        let body_bytes = response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec();
        String::from_utf8(body_bytes).unwrap()
    }

    #[tokio::test]
    async fn test_request_messages_sent_to_app_ok() {
        let handler = HTTPHandler::new(Duration::from_secs(10));
        let request = build_request("");
        let send_to = SendToAppCollector::new();
        let receive_from = DeterministicReceiveFromApp::default();

        let _ = handler
            .handle(send_to.clone(), receive_from, request)
            .await
            .unwrap();

        let messages = send_to.get_messages().await;

        assert!(messages.len() == 2);
    }

    #[tokio::test]
    async fn test_sent_body_is_ok() {
        let handler = HTTPHandler::new(Duration::from_secs(10));
        let request = build_request("hello world");
        let send_to = SendToAppCollector::new();
        let receive_from = DeterministicReceiveFromApp::default();

        let _ = handler
            .handle(send_to.clone(), receive_from, request)
            .await
            .unwrap();

        let messages = send_to.get_messages().await;

        // read the body from the messages
        let mut body = String::new();
        for message in &messages {
            match message {
                ASGIReceiveEvent::HTTPRequest(msg) => {
                    body.push_str(&String::from_utf8_lossy(&msg.body));
                }
                ASGIReceiveEvent::HTTPDisconnect(_) => {}
                msg => panic!("Unexpected ASGI message received: {:?}", msg),
            }
        }

        assert!(body == "hello world");
    }

    #[tokio::test]
    async fn test_send_fails() {
        let handler = HTTPHandler::new(Duration::from_secs(10));
        let request = build_request("hello world");
        let send_to = SendToAppFail::new();
        let receive_from = DeterministicReceiveFromApp::default();

        let response = handler.handle(send_to, receive_from, request).await;

        assert!(response.is_err_and(|e| { e.to_string() == "SendToAppFail always fails" }));
    }

    #[tokio::test]
    async fn test_last_message_is_disconnect() {
        let handler = HTTPHandler::new(Duration::from_secs(10));
        let request = build_request("hello world");
        let send_to = SendToAppCollector::new();
        let receive_from = DeterministicReceiveFromApp::default();

        let _ = handler
            .handle(send_to.clone(), receive_from, request)
            .await
            .unwrap();

        let messages = send_to.get_messages().await;
        assert!(messages[messages.len() - 1] == ASGIReceiveEvent::new_http_disconnect());
    }

    #[tokio::test]
    async fn test_response_ok() {
        let handler = HTTPHandler::new(Duration::from_secs(10));
        let request = build_request("");
        let send_to = SendToAppCollector::new();
        let receive_from = DeterministicReceiveFromApp::new(vec![
            Ok(ASGISendEvent::new_http_response_start(200, vec![])),
            Ok(ASGISendEvent::new_http_response_body(
                Bytes::from("app sent this body"),
                false,
            )),
        ]);

        let response = handler
            .handle(send_to, receive_from, request)
            .await
            .unwrap();

        assert!(response.status() == http::StatusCode::OK);
        assert!(response_to_body_string(response).await == "app sent this body");
    }

    #[tokio::test]
    async fn test_body_sent_in_parts() {
        let handler = HTTPHandler::new(Duration::from_secs(10));
        let request = build_request("");
        let send_to = SendToAppCollector::new();
        let receive_from = DeterministicReceiveFromApp::new(vec![
            Ok(ASGISendEvent::new_http_response_start(200, vec![])),
            Ok(ASGISendEvent::new_http_response_body(
                Bytes::from("part 1 "),
                true,
            )),
            Ok(ASGISendEvent::new_http_response_body(
                Bytes::from("part 2"),
                false,
            )),
        ]);

        let response = handler
            .handle(send_to, receive_from, request)
            .await
            .unwrap();

        assert!(response.status() == http::StatusCode::OK);
        assert!(response_to_body_string(response).await == "part 1 part 2");
    }

    #[tokio::test]
    async fn test_headers_ok() {
        let header_key_1 = Bytes::from("test");
        let header_value_1 = Bytes::from("header");
        let header_key_2 = Bytes::from("another");
        let header_value_2 = Bytes::from("header");

        let handler = HTTPHandler::new(Duration::from_secs(10));
        let request = build_request("");
        let send_to = SendToAppCollector::new();
        let receive_from = DeterministicReceiveFromApp::new(vec![
            Ok(ASGISendEvent::new_http_response_start(
                200,
                vec![
                    (header_key_1, header_value_1),
                    (header_key_2, header_value_2),
                ],
            )),
            Ok(ASGISendEvent::new_http_response_body(
                Bytes::from(""),
                false,
            )),
        ]);

        let response = handler
            .handle(send_to, receive_from, request)
            .await
            .unwrap();

        assert!(response.status() == http::StatusCode::OK);
        let headers = response.headers();
        assert!(headers.get("test").and_then(|v| Some(v.to_str().unwrap())) == Some("header"));
        assert!(
            headers
                .get("another")
                .and_then(|v| Some(v.to_str().unwrap()))
                == Some("header")
        );
    }

    #[tokio::test]
    async fn test_receive_from_fails() {
        let handler = HTTPHandler::new(Duration::from_secs(10));
        let request = build_request("");
        let send_to = SendToAppCollector::new();
        let receive_from =
            DeterministicReceiveFromApp::new(vec![Err(ArasError::custom("ReceiveFromApp failed").into())]);

        let response = handler.handle(send_to, receive_from, request).await;

        assert!(response.is_err_and(|e| { e.to_string() == "ReceiveFromApp failed" }));
    }

    #[tokio::test]
    async fn test_receive_from_fails_while_streaming_body() {
        let handler = HTTPHandler::new(Duration::from_secs(10));
        let request = build_request("");
        let send_to = SendToAppCollector::new();
        let receive_from = DeterministicReceiveFromApp::new(vec![
            Ok(ASGISendEvent::new_http_response_start(200, vec![])),
            Ok(ASGISendEvent::new_http_response_body(
                Bytes::from("part 1 "),
                true,
            )),
            Err(ArasError::custom("ReceiveFromApp failed").into()),
        ]);

        let response = handler.handle(send_to, receive_from, request).await;

        // Response is ok, error occurs while streaming body
        assert!(response.is_ok());

        let body = response.unwrap().into_body().collect().await;
        assert!(body.is_err_and(|e| { e.to_string() == "ReceiveFromApp failed" }));
    }

    #[tokio::test]
    async fn test_invalid_asgi_message_received_while_streaming_body() {
        let handler = HTTPHandler::new(Duration::from_secs(10));
        let request = build_request("");
        let send_to = SendToAppCollector::new();
        let receive_from = DeterministicReceiveFromApp::new(vec![
            Ok(ASGISendEvent::new_http_response_start(200, vec![])),
            Ok(ASGISendEvent::new_http_response_body(
                Bytes::from("part 1 "),
                true,
            )),
            Ok(ASGISendEvent::new_shutdown_complete()),
        ]);

        let response = handler.handle(send_to, receive_from, request).await;

        // Response is ok, error occurs while streaming body
        assert!(response.is_ok());

        let body = response.unwrap().into_body().collect().await;
        assert!(body.is_err_and(|e| {
            e.to_string() == "Unexpected ASGI message received. ShutdownComplete(LifespanShutdownCompleteEvent)"
        }));
    }

    #[tokio::test]
    async fn test_backpressure_timeout() {
        let handler = HTTPHandler::new(Duration::from_millis(1));
        let request = build_request("");
        let send_to = SendToAppCollector::new();
        let receive_from = DeterministicReceiveFromApp::new(vec![]);

        let response = handler.handle(send_to, receive_from, request).await;

        assert!(response.is_err_and(|e| e.to_string() == "ASGI await timeout elapsed"));
    }
}
