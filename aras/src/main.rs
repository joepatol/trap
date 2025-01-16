use std::time::Duration;
use asgispec::prelude::*;
use aras::ArasServer;
use log::LevelFilter;
use simplelog::*;

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

const HEADERS: [(&str, &str); 1] = [("content-type", "text/html")];

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
                println!("rec HTTP scope");
                let msg = receive().await;
                println!("{msg}");
                let headers = HEADERS.into_iter().map(|(k, v)| (k.as_bytes().to_vec(), v.as_bytes().to_vec())).collect();
                send(ASGISendEvent::new_http_response_start(200, headers)).await.unwrap();
                send(ASGISendEvent::new_http_response_body(HTML_SIMPLE.as_bytes().to_vec(), false)).await.unwrap();
            },
            Scope::Lifespan(_) => {
                println!("rec lifespan scope");
                let _ = receive().await;
                send(ASGISendEvent::new_startup_complete()).await.unwrap();
            },
            Scope::Websocket(_) => {
                println!("rec Websocket scope");
                let msg = receive().await;
                println!("{msg}");
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

#[tokio::main]
async fn main() {
    SimpleLogger::init(LevelFilter::Info, Config::default()).unwrap();

    let server = ArasServer::new([127, 0, 0, 1].into(), 8080, true, Duration::from_secs(60), 1_000_000_000, 1000);
    let application = WsApp {};
    let state = MockState {};

    let _ = server.run(application, state);
}