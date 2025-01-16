// ASGI Applications used for testing

use asgispec::prelude::*;

// Mocks
#[derive(Debug)]
pub struct TestError(String);

impl std::fmt::Display for TestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)?;
        Ok(())
    }
}

impl From<Box<dyn std::error::Error>> for TestError {
    fn from(value: Box<dyn std::error::Error>) -> Self {
        Self { 0: value.to_string() }
    }
}

impl std::error::Error for TestError {}

#[derive(Clone, Debug)]
pub struct MockState;
impl State for MockState {}
impl std::fmt::Display for MockState {
    fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Ok(())
    }
}

// Actual applications
#[derive(Clone, Debug)]
pub struct EchoApp {
    extra_body: Option<String>,
}

impl EchoApp {
    pub fn new() -> Self {
        Self { extra_body: None }
    }

    pub fn new_with_body(body: &str) -> Self {
        Self {
            extra_body: Some(body.to_string()),
        }
    }
}

impl ASGIApplication for EchoApp {
    type State = MockState;
    type Error = TestError;

    async fn call(&self, _scope: Scope<Self::State>, receive: ReceiveFn, send: SendFn) -> Result<(), Self::Error> {
        let mut body = Vec::new();
        let headers = Vec::from([
            ("test".as_bytes().to_vec(), "header".as_bytes().to_vec()),
            ("another".as_bytes().to_vec(), "header".as_bytes().to_vec()),
        ]);
        loop {
            match (receive)().await {
                ASGIReceiveEvent::HTTPRequest(msg) => {
                    body.extend(msg.body.into_iter());
                    if msg.more_body {
                        continue;
                    } else {
                        let start_msg = ASGISendEvent::new_http_response_start(200, headers.clone());
                        send(start_msg).await.map_err(|e| TestError { 0: e.to_string() })?;
                        let more_body = self.extra_body.is_some();
                        let body_msg = ASGISendEvent::new_http_response_body(body.clone(), more_body);
                        send(body_msg).await.map_err(|e| TestError { 0: e.to_string() })?;
                        if let Some(b) = &self.extra_body {
                            let next_msg =
                                ASGISendEvent::new_http_response_body(b.to_string().as_bytes().to_vec(), false);
                            (send)(next_msg).await.map_err(|e| TestError { 0: e.to_string() })?;
                        };
                    };
                }
                ASGIReceiveEvent::HTTPDisconnect(_) => {
                    break;
                }
                _ => return Err(TestError { 0: "Invalid message received from server".into() }),
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct ImmediateReturnApp;

impl ASGIApplication for ImmediateReturnApp {
    type Error = TestError;
    type State = MockState;

    async fn call(&self, _scope: Scope<MockState>, _receive: ReceiveFn, _send: SendFn) -> Result<(), Self::Error> {
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct ErrorOnCallApp;

impl ASGIApplication for ErrorOnCallApp {
    type Error = TestError;
    type State = MockState;

    async fn call(&self, _scope: Scope<Self::State>, _receive: ReceiveFn, _send: SendFn) -> Result<(), Self::Error> {
        Err(TestError { 0: "Immediate error".into() })
    }
}

#[derive(Clone, Debug)]
pub struct ErrorInLoopApp;

impl ASGIApplication for ErrorInLoopApp {
    type Error = TestError;
    type State = MockState;
    
    async fn call(&self, _scope: Scope<Self::State>, receive: ReceiveFn, _send: SendFn) -> Result<(), Self::Error> {
        _ = receive().await;
        Err(TestError { 0: "Error in loop".into() })
    }
}

#[derive(Clone, Debug)]
pub struct ErrorInBodyLoopApp;

impl ASGIApplication for ErrorInBodyLoopApp {
    type Error = TestError;
    type State = MockState;

    async fn call(&self, _scope: Scope<Self::State>, receive: ReceiveFn, send: SendFn) -> Result<(), Self::Error> {
        _ = receive().await;
        send(ASGISendEvent::new_http_response_start(200, Vec::new())).await.map_err(|e| TestError { 0: e.to_string() })?;
        Err(TestError { 0: "Error in loop".into() })
    }
}

#[derive(Clone, Debug)]
pub struct AssertSendErrorApp;

impl ASGIApplication for AssertSendErrorApp {
    type Error = TestError;
    type State = MockState;
    
    async fn call(&self, _scope: Scope<Self::State>, receive: ReceiveFn, send: SendFn) -> Result<(), Self::Error> {
        loop {
            let _ = receive().await;
            return send(ASGISendEvent::new_shutdown_complete()).await.map_err(|e| TestError { 0: e.to_string() })
        }
    }
}

#[derive(Clone, Debug)]
pub struct ErrorInDataStreamApp;

impl ASGIApplication for ErrorInDataStreamApp {
    type Error = TestError;
    type State = MockState;
    
    async fn call(&self, _scope: Scope<Self::State>, receive: ReceiveFn, send: SendFn) -> Result<(), Self::Error> {
        let headers = Vec::from([(
            String::from("a").as_bytes().to_vec(),
            String::from("header").as_bytes().to_vec(),
        )]);
        _ = receive().await;
        let res_start_msg = ASGISendEvent::new_http_response_start(200, headers);
        send(res_start_msg).await.map_err(|e| TestError { 0: e.to_string() })?;
        let first_body = ASGISendEvent::new_http_response_body(String::from("hello").as_bytes().to_vec(), true);
        send(first_body).await.map_err(|e| TestError { 0: e.to_string() })?;
        // Instead of more body an invalid message is sent to mimick the error
        let invalid = ASGISendEvent::new_startup_complete();
        send(invalid).await.map_err(|e| TestError { 0: e.to_string() })?;
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct LifespanApp;

impl ASGIApplication for LifespanApp {
    type Error = TestError;
    type State = MockState;

    async fn call(&self, scope: Scope<Self::State>, receive: ReceiveFn, send: SendFn) -> Result<(), Self::Error> {
        if let Scope::Lifespan(_) = scope {
            loop {
                match receive().await {
                    ASGIReceiveEvent::Startup(_) => {
                        send(ASGISendEvent::new_startup_complete()).await.map_err(|e| TestError { 0: e.to_string() })?;
                    }
                    ASGIReceiveEvent::Shutdown(_) => return send(ASGISendEvent::new_shutdown_complete()).await.map_err(|e| TestError { 0: e.to_string() }),
                    _ => return Err(TestError { 0: "Invalid message".into() }),
                }
            }
        };
        Err(TestError { 0: "Invalid message".into() })
    }
}

#[derive(Clone, Debug)]
pub struct LifespanUnsupportedApp;

impl ASGIApplication for LifespanUnsupportedApp {
    type Error = TestError;
    type State = MockState;

    async fn call(&self, scope: Scope<Self::State>, receive: ReceiveFn, send: SendFn) -> Result<(), Self::Error> {
        if let Scope::Lifespan(_) = scope {
            loop {
                _ = receive().await;
                // Send an unrelated message, to mimick the protocol not being supported
                send(ASGISendEvent::new_http_response_body("oops".into(), false)).await.map_err(|e| TestError { 0: e.to_string() })?;
            }
        };
        Err(TestError { 0: "Invalid scope".into() })
    }
}

#[derive(Clone, Debug)]
pub struct LifespanFailedApp;

impl ASGIApplication for LifespanFailedApp {
    type Error = TestError;
    type State = MockState;

    async fn call(&self, scope: Scope<Self::State>, receive: ReceiveFn, send: SendFn) -> Result<(), Self::Error> {
        if let Scope::Lifespan(_) = scope {
            loop {
                match receive().await {
                    ASGIReceiveEvent::Startup(_) => {
                        send(ASGISendEvent::new_startup_failed("test".to_string())).await.map_err(|e| TestError { 0: e.to_string() })?;
                    }
                    ASGIReceiveEvent::Shutdown(_) => {
                        return send(ASGISendEvent::new_shutdown_failed("test".to_string())).await.map_err(|e| TestError { 0: e.to_string() });
                    }
                    _ => return Err(TestError { 0: "Invalid scope".into() }),
                }
            }
        };
        Err(TestError { 0: "Invalid scope".into() })
    }
}

const HTML_SIMPLE: &str = r#"
<!DOCTYPE html>
<html>
    <head>
        <title>Chat</title>
    </head>
    <body>
        <h1>WebSocket Chat</h1>
        <form action="" onsubmit="sendMessage(event)">
            <input type="text" id="messageText" autocomplete="off"/>
            <button>Send</button>
        </form>
        <ul id='messages'>
        </ul>
        <script>
            var ws = new WebSocket("ws://localhost:8080/api/chat/ws_echo");
            ws.onmessage = function(event) {
                var messages = document.getElementById('messages')
                var message = document.createElement('li')
                var content = document.createTextNode(event.data)
                message.appendChild(content)
                messages.appendChild(message)
            };
            function sendMessage(event) {
                var input = document.getElementById("messageText")
                ws.send(input.value)
                input.value = ''
                event.preventDefault()
            }
        </script>
    </body>
</html>
"#;

const HEADERS: [(&str, &str); 1] = [("content-type", "application/json")];

#[derive(Clone, Debug)]
pub struct WsApp;

impl ASGIApplication for WsApp {
    type Error = TestError;
    type State = MockState;

    async fn call(
            &self,
            scope: Scope<Self::State>,
            receive: ReceiveFn,
            send: SendFn,
        ) -> Result<(), Self::Error> {
        match scope {
            Scope::HTTP(_) => {
                let _ = receive().await;
                let headers = HEADERS.into_iter().map(|(k, v)| (k.as_bytes().to_vec(), v.as_bytes().to_vec())).collect();
                send(ASGISendEvent::new_http_response_start(200, headers)).await.unwrap();
                send(ASGISendEvent::new_http_response_body(HTML_SIMPLE.as_bytes().to_vec(), false)).await.unwrap();
            },
            Scope::Lifespan(_) => {
                let _ = receive().await;
                send(ASGISendEvent::new_startup_complete()).await.unwrap();
            },
            Scope::Websocket(_) => {
                let _ = receive().await;
                send(ASGISendEvent::new_websocket_accept(None, Vec::new())).await.unwrap();

                loop {
                    if let ASGIReceiveEvent::WebsocketReceive(msg) = receive().await {
                        send(ASGISendEvent::new_websocket_send(msg.bytes, msg.text)).await.unwrap();
                    } else {
                        panic!("unexpected ASGI msg from server")
                    }
                }
            }
        };

        Ok(())
    }
}