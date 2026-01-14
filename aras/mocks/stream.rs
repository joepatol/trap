use fastwebsockets::{Frame, Payload};
use std::collections::VecDeque;
use std::io;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

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
        };
        data_buffer.push_back(create_close_frame());

        MockWebsocketStream {
            written,
            data_buffer: Arc::new(Mutex::new(data_buffer)),
        }
    }

    pub fn written(&self) -> Vec<Vec<u8>> {
        let guard = self.written.lock().unwrap();
        guard.clone()
    }

    pub fn written_unmasked(&self) -> Result<Vec<String>, String> {
        let mut unmasked_frames = Vec::new();
        for frame in self.written().iter() {
            let unmasked = parse_and_unmask_frame(frame)?;
            unmasked_frames.push(unmasked);
        }
        Ok(unmasked_frames)
    }
}

impl AsyncRead for MockWebsocketStream {
    fn poll_read(self: Pin<&mut Self>, _: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
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
    fn poll_write(self: Pin<&mut Self>, _: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
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

pub fn parse_and_unmask_frame(frame: &[u8]) -> Result<String, String> {
    if frame.len() < 2 {
        return Err("frame too short (missing header)".into());
    }

    let b1 = frame[1];

    let masked = (b1 & 0b1000_0000) != 0;
    if !masked {
        return Err("expected client frame to be masked (MASK bit not set)".into());
    }

    let mut idx = 2usize;
    let len_indicator = (b1 & 0x7F) as usize;

    // Determine payload length and where the mask key starts
    let payload_len = match len_indicator {
        n @ 0..=125 => n,
        126 => {
            if frame.len() < idx + 2 {
                return Err("frame too short (missing 16-bit extended length)".into());
            }
            let v = u16::from_be_bytes([frame[idx], frame[idx + 1]]) as usize;
            idx += 2;
            v
        }
        127 => {
            if frame.len() < idx + 8 {
                return Err("frame too short (missing 64-bit extended length)".into());
            }
            let v = u64::from_be_bytes([
                frame[idx],
                frame[idx + 1],
                frame[idx + 2],
                frame[idx + 3],
                frame[idx + 4],
                frame[idx + 5],
                frame[idx + 6],
                frame[idx + 7],
            ]);
            idx += 8;

            v as usize
        }
        _ => unreachable!(),
    };

    // Need 4 bytes of masking key
    if frame.len() < idx + 4 {
        return Err("frame too short (missing masking key)".into());
    }

    let mask_key = [frame[idx], frame[idx + 1], frame[idx + 2], frame[idx + 3]];
    idx += 4;

    // Ensure we have the full masked payload
    if frame.len() < idx + payload_len {
        return Err("frame too short (truncated payload)".into());
    }

    let masked_payload = &frame[idx..idx + payload_len];

    // Unmask
    let mut payload_unmasked = Vec::with_capacity(payload_len);
    for (i, &b) in masked_payload.iter().enumerate() {
        payload_unmasked.push(b ^ mask_key[i % 4]);
    }

    Ok(String::from_utf8_lossy(&payload_unmasked).to_string())
}
