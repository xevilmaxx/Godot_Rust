//! Length-prefixed TCP transport for `MultiplayerApi` (loopback-tested).
//!
//! Frames are `u32 BE length + JSON payload`, so any Godot-side socket peer can
//! speak the same framing. Sockets are non-blocking; `poll()` drains whatever
//! has arrived. `TcpLink::connect` / `TcpHost::accept_one` cover client and
//! server roles used by `MultiplayerApi` loopback tests and real LAN play.

use godot_core::GodotError;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::time::Duration;

const MAX_FRAME: usize = 4 * 1024 * 1024;

/// One side of a connection. Owns a read buffer; call `poll()` per tick.
pub struct TcpLink {
    stream: TcpStream,
    peer: SocketAddr,
    buf: Vec<u8>,
}

impl TcpLink {
    pub fn connect(addr: &str) -> Result<Self, GodotError> {
        let stream = TcpStream::connect(addr).map_err(|e| GodotError::Io(format!("connect {addr}: {e}")))?;
        stream.set_nonblocking(true).map_err(|e| GodotError::Io(e.to_string()))?;
        stream.set_nodelay(true).ok();
        let peer = stream.peer_addr().map_err(|e| GodotError::Io(e.to_string()))?;
        Ok(Self { stream, peer, buf: vec![] })
    }

    fn from_stream(stream: TcpStream) -> Result<Self, GodotError> {
        stream.set_nonblocking(true).map_err(|e| GodotError::Io(e.to_string()))?;
        stream.set_nodelay(true).ok();
        let peer = stream.peer_addr().map_err(|e| GodotError::Io(e.to_string()))?;
        Ok(Self { stream, peer, buf: vec![] })
    }

    pub fn peer(&self) -> SocketAddr {
        self.peer
    }

    pub fn send(&mut self, payload: &[u8]) -> Result<(), GodotError> {
        if payload.len() > MAX_FRAME {
            return Err(GodotError::InvalidArgument(format!("frame too big: {}", payload.len())));
        }
        let len = (payload.len() as u32).to_be_bytes();
        self.stream.write_all(&len).and_then(|_| self.stream.write_all(payload)).map_err(|e| GodotError::Io(format!("send: {e}")))?;
        Ok(())
    }

    /// Drain complete frames (may be empty). Keeps partial tails buffered.
    pub fn poll(&mut self) -> Result<Vec<Vec<u8>>, GodotError> {
        let mut tmp = [0u8; 8192];
        loop {
            match self.stream.read(&mut tmp) {
                Ok(0) => break, // orderly shutdown (or no data yet on some stacks)
                Ok(n) => self.buf.extend_from_slice(&tmp[..n]),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => return Err(GodotError::Io(format!("recv: {e}"))),
            }
            if self.buf.len() > MAX_FRAME + 4 {
                return Err(GodotError::InvalidArgument("peer sent oversize frame".into()));
            }
            // Non-blocking readers may deliver everything at once; loop once more.
            // (Single extra pass is enough for loopback tests and LAN ticks.)
            break;
        }
        let mut out = vec![];
        while self.buf.len() >= 4 {
            let len = u32::from_be_bytes([self.buf[0], self.buf[1], self.buf[2], self.buf[3]]) as usize;
            if len > MAX_FRAME {
                return Err(GodotError::InvalidArgument("peer sent oversize frame".into()));
            }
            if self.buf.len() < 4 + len {
                break;
            }
            out.push(self.buf[4..4 + len].to_vec());
            self.buf.drain(..4 + len);
        }
        Ok(out)
    }
}

/// Listening side. `accept_one` blocks up to `timeout` for the next client.
pub struct TcpHost {
    listener: TcpListener,
}

impl TcpHost {
    pub fn bind_loopback() -> Result<(Self, SocketAddr), GodotError> {
        let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| GodotError::Io(format!("bind: {e}")))?;
        let addr = listener.local_addr().map_err(|e| GodotError::Io(e.to_string()))?;
        listener.set_nonblocking(true).map_err(|e| GodotError::Io(e.to_string()))?;
        Ok((Self { listener }, addr))
    }

    pub fn accept_one(&self, timeout: Duration) -> Result<TcpLink, GodotError> {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            match self.listener.accept() {
                Ok((stream, _)) => return TcpLink::from_stream(stream),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    if std::time::Instant::now() >= deadline {
                        return Err(GodotError::Timeout(format!("no client within {timeout:?}")));
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(e) => return Err(GodotError::Io(format!("accept: {e}"))),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn loopback_frames_roundtrip_in_order() {
        let (host, addr) = TcpHost::bind_loopback().unwrap();
        let mut client = TcpLink::connect(&addr.to_string()).unwrap();
        let mut server = host.accept_one(Duration::from_secs(5)).unwrap();
        client.send(br#"{"rpc":"ping"}"#).unwrap();
        client.send(&vec![7u8; 3000]).unwrap();
        // Let loopback deliver, then drain both ends.
        std::thread::sleep(Duration::from_millis(50));
        let got = server.poll().unwrap();
        assert_eq!(got.len(), 2, "both frames arrive, got {}", got.len());
        assert_eq!(got[0], br#"{"rpc":"ping"}"#);
        assert_eq!(got[1].len(), 3000);
        // Replies flow back.
        server.send(b"pong").unwrap();
        std::thread::sleep(Duration::from_millis(50));
        let back = client.poll().unwrap();
        assert_eq!(back, vec![b"pong".to_vec()]);
    }
    #[test]
    fn oversize_frames_are_rejected() {
        let (host, addr) = TcpHost::bind_loopback().unwrap();
        let mut client = TcpLink::connect(&addr.to_string()).unwrap();
        let _server = host.accept_one(Duration::from_secs(5)).unwrap();
        assert!(client.send(&vec![0u8; MAX_FRAME + 1]).is_err());
    }
}
