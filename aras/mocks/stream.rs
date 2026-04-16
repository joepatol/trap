use fastwebsockets::{Frame, Payload, OpCode, Role, WebSocket};
use std::collections::VecDeque;
use std::io;
use std::io::Cursor;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

pub(crate) trait FrameExt {
    fn is_close_frame(&self) -> bool;
    fn close_code(&self) -> Option<u16>;
    fn message(&self) -> Option<String>;
}

impl FrameExt for Frame<'_> {
    fn is_close_frame(&self) -> bool {
        self.opcode == OpCode::Close
    }

    fn close_code(&self) -> Option<u16> {
        if self.is_close_frame() && self.payload.len() >= 2 {
            let code_bytes = &self.payload[0..2];
            Some(u16::from_be_bytes([code_bytes[0], code_bytes[1]]))
        } else {
            None
        }
    }

    fn message(&self) -> Option<String> {
        let msg_bytes = if self.is_close_frame() && self.payload.len() > 2 {
            Some(&self.payload[2..])
        } else if self.is_close_frame() {
            None
        } else {
            Some(&self.payload[..])
        };

        String::from_utf8(msg_bytes?.to_vec()).ok()
    }
}

#[derive(Clone, Debug)]
pub(crate) struct MockWebsocketStream {
    written: Arc<Mutex<Vec<Vec<u8>>>>,
    data_buffer: Arc<Mutex<VecDeque<Vec<u8>>>>,
}

impl Unpin for MockWebsocketStream {}

impl MockWebsocketStream {
    pub fn empty() -> MockWebsocketStream {
        MockWebsocketStream {
            written: Arc::new(Mutex::new(Vec::new())),
            data_buffer: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    pub fn new(messages: Vec<&str>) -> MockWebsocketStream {
        let written = Arc::new(Mutex::new(Vec::new()));
        let mut data_buffer = VecDeque::new();

        for message in messages.into_iter() {
            data_buffer.push_back(create_frame(message));
        }
        data_buffer.push_back(create_close_frame());

        MockWebsocketStream {
            written,
            data_buffer: Arc::new(Mutex::new(data_buffer)),
        }
    }

    fn written(&self) -> Vec<Vec<u8>> {
        let guard = self.written.lock().unwrap();
        guard.clone()
    }

    pub async fn written_frames(&self) -> Vec<Frame<'static>> {
        let all_bytes: Vec<u8> = self.written().into_iter().flatten().collect();
        let cursor = Cursor::new(all_bytes);
        let mut ws = WebSocket::after_handshake(cursor, Role::Server);
        ws.set_auto_close(false);

        let mut frames = Vec::new();
        loop {
            match ws.read_frame().await {
                Ok(frame) => {
                    let payload = Payload::Owned(frame.payload.to_vec());
                    frames.push(Frame::new(frame.fin, frame.opcode, None, payload));
                }
                Err(_) => break,
            }
        }
        frames
    }
}

impl AsyncRead for MockWebsocketStream {
    fn poll_read(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();

        let mut guard = this.data_buffer.lock().unwrap();
        let msg = guard.pop_front();

        if msg.is_none() {
            return Poll::Pending;
        } else {
            buf.put_slice(&msg.as_ref().unwrap());
        }

        Poll::Ready(Ok(()))
    }
}

impl AsyncWrite for MockWebsocketStream {
    fn poll_write(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();

        let mut guard = this.written.lock().unwrap();
        guard.push(buf.to_vec());

        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        Poll::Ready(Ok(()))
    }
}

fn create_close_frame() -> Vec<u8> {
    let mut frame = Frame::close(1000, &[]);
    let mut buf = Vec::new();
    frame.write(&mut buf).into()
}

fn create_frame(text: &str) -> Vec<u8> {
    let mut frame = Frame::text(Payload::Owned(text.into()));
    let mut buf = Vec::new();
    frame.write(&mut buf).into()
}
