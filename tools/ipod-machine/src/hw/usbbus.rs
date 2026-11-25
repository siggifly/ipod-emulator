//! The virtual USB bus of the device controller: a framed protocol on a Unix domain socket.
//!
//! The emulator is the device end and listens; a test or tool is the host end. One host is
//! connected at a time, and a new connection replaces the old one. Every frame is an 8-byte
//! header followed by `len` payload bytes:
//!
//! ```text
//! kind addr ep aux flags rsvd len(u16 LE)
//! ```
//!
//! The host sends one request and waits for the answer to it; `STATE` frames may arrive between.
//! The reader runs on its own thread so that the run loop only tests an atomic per service tick.

use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const HEADER: usize = 8;
/// Largest payload one frame carries; a high-speed bulk packet is 512 bytes.
pub const MAX_PAYLOAD: usize = 4096;

/// Host to device.
pub const ATTACH: u8 = 0x01;
pub const DETACH: u8 = 0x02;
pub const RESET: u8 = 0x03;
pub const SETUP: u8 = 0x04;
pub const OUT: u8 = 0x05;
pub const IN: u8 = 0x06;
pub const QUIT: u8 = 0x07;
pub const GET_STATE: u8 = 0x08;
/// Device to host.
pub const STATE: u8 = 0x81;
pub const RESULT: u8 = 0x82;

/// `RESULT.aux`.
pub const ACK: u8 = 0;
pub const NAK: u8 = 1;
pub const STALL: u8 = 2;
pub const TIMEOUT: u8 = 3;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub kind: u8,
    pub addr: u8,
    pub ep: u8,
    pub aux: u8,
    pub flags: u8,
    pub data: Vec<u8>,
}

impl Frame {
    pub fn new(kind: u8) -> Frame {
        Frame {
            kind,
            addr: 0,
            ep: 0,
            aux: 0,
            flags: 0,
            data: Vec::new(),
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(HEADER + self.data.len());
        v.extend_from_slice(&[self.kind, self.addr, self.ep, self.aux, self.flags, 0]);
        v.extend_from_slice(&(self.data.len() as u16).to_le_bytes());
        v.extend_from_slice(&self.data);
        v
    }

    pub fn read_from(r: &mut impl Read) -> io::Result<Frame> {
        let mut h = [0u8; HEADER];
        r.read_exact(&mut h)?;
        let len = u16::from_le_bytes([h[6], h[7]]) as usize;
        if len > MAX_PAYLOAD {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "frame too long"));
        }
        let mut data = vec![0u8; len];
        r.read_exact(&mut data)?;
        Ok(Frame {
            kind: h[0],
            addr: h[1],
            ep: h[2],
            aux: h[3],
            flags: h[4],
            data,
        })
    }
}

#[derive(Debug)]
pub enum Event {
    Connected,
    Frame(Frame),
    Disconnected,
}

struct Shared {
    queue: Mutex<VecDeque<(u64, Event)>>,
    flag: AtomicBool,
    out: Mutex<Option<UnixStream>>,
    epoch: AtomicU64,
}

pub struct Link {
    shared: Arc<Shared>,
}

impl Link {
    /// Bind `path` (replacing a stale socket file) and start accepting.
    pub fn listen(path: &Path) -> io::Result<Link> {
        let _ = std::fs::remove_file(path);
        let listener = UnixListener::bind(path)?;
        let shared = Arc::new(Shared {
            queue: Mutex::new(VecDeque::new()),
            flag: AtomicBool::new(false),
            out: Mutex::new(None),
            epoch: AtomicU64::new(0),
        });
        let s = shared.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let epoch = s.epoch.fetch_add(1, Ordering::SeqCst) + 1;
                let _ = stream.set_write_timeout(Some(Duration::from_secs(10)));
                if let Ok(w) = stream.try_clone() {
                    let mut out = s.out.lock().unwrap();
                    if let Some(old) = out.take() {
                        let _ = old.shutdown(std::net::Shutdown::Both);
                    }
                    *out = Some(w);
                }
                push(&s, epoch, Event::Connected);
                let s2 = s.clone();
                std::thread::spawn(move || {
                    let mut r = stream;
                    while let Ok(f) = Frame::read_from(&mut r) {
                        push(&s2, epoch, Event::Frame(f));
                    }
                    push(&s2, epoch, Event::Disconnected);
                });
            }
        });
        Ok(Link { shared })
    }

    /// Whether anything is waiting; one atomic load.
    #[inline]
    pub fn pending(&self) -> bool {
        self.shared.flag.load(Ordering::Acquire)
    }

    /// The next event of the current connection. Events of a replaced connection are dropped.
    pub fn pop(&self) -> Option<Event> {
        let mut q = self.shared.queue.lock().unwrap();
        let cur = self.shared.epoch.load(Ordering::SeqCst);
        loop {
            match q.pop_front() {
                Some((e, ev)) if e == cur => {
                    if q.is_empty() {
                        self.shared.flag.store(false, Ordering::Release);
                    }
                    return Some(ev);
                }
                Some(_) => {}
                None => {
                    self.shared.flag.store(false, Ordering::Release);
                    return None;
                }
            }
        }
    }

    pub fn send(&self, f: &Frame) {
        if let Some(w) = self.shared.out.lock().unwrap().as_mut() {
            let _ = w.write_all(&f.encode());
        }
    }
}

fn push(s: &Shared, epoch: u64, ev: Event) {
    s.queue.lock().unwrap().push_back((epoch, ev));
    s.flag.store(true, Ordering::Release);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_survives_the_wire() {
        let f = Frame {
            kind: OUT,
            addr: 5,
            ep: 2,
            aux: 1,
            flags: 1,
            data: vec![1, 2, 3],
        };
        let b = f.encode();
        assert_eq!(b.len(), HEADER + 3);
        assert_eq!(Frame::read_from(&mut &b[..]).unwrap(), f);
    }

    #[test]
    fn an_oversized_length_is_refused() {
        let mut b = Frame::new(OUT).encode();
        b[6] = 0xff;
        b[7] = 0xff;
        assert!(Frame::read_from(&mut &b[..]).is_err());
    }
}
