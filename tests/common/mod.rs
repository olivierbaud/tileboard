//! Shared by the test files: start a server on a free loopback port, and call it over a real socket.
#![allow(dead_code)]

use std::future::Future;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};

use serde_json::Value;

/// Runs `serve` on its own thread, on a free loopback port, and returns the address.
pub fn start<F, Fut>(serve: F) -> SocketAddr
where
    F: FnOnce(tokio::net::TcpListener) -> Fut + Send + 'static,
    Fut: Future<Output = std::io::Result<()>>,
{
    // Bound before the server thread starts, so early requests wait instead of failing.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        runtime.block_on(async move {
            let listener = tokio::net::TcpListener::from_std(listener).unwrap();
            serve(listener).await.unwrap();
        });
    });
    addr
}

/// An address on loopback where nothing listens.
pub fn closed_port() -> SocketAddr {
    TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap()
}

pub struct Answer {
    pub status: u16,
    /// The status line and headers, in lowercase.
    pub head: String,
    pub body: String,
}

impl Answer {
    pub fn json(&self) -> Value {
        assert!(self.head.contains("content-type: application/json"), "not JSON: {}", self.head);
        serde_json::from_str(&self.body).unwrap_or_else(|e| panic!("body is not JSON: {e}\n{}", self.body))
    }

    /// The JSON body of an answer that must be a 200.
    pub fn ok_json(&self) -> Value {
        assert_eq!(self.status, 200, "{}", self.body);
        self.json()
    }
}

/// One plain HTTP/1.1 request. `Connection: close` lets us read until the end.
pub fn request(addr: SocketAddr, method: &str, path: &str, body: Option<(&str, &str)>) -> Answer {
    let mut stream = TcpStream::connect(addr).unwrap();
    let mut text = format!("{method} {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n");
    if let Some((content_type, body)) = body {
        text.push_str(&format!("Content-Type: {content_type}\r\nContent-Length: {}\r\n\r\n{body}", body.len()));
    } else {
        text.push_str("\r\n");
    }
    stream.write_all(text.as_bytes()).unwrap();
    let mut raw = String::new();
    stream.read_to_string(&mut raw).unwrap();

    let (head, body) = raw.split_once("\r\n\r\n").unwrap_or_else(|| panic!("no header end in {raw:?}"));
    let head = head.to_ascii_lowercase();
    assert!(!head.contains("transfer-encoding"), "this small client cannot read chunked answers");
    let status = head.split(' ').nth(1).and_then(|code| code.parse().ok()).expect("a status code");
    Answer { status, head, body: body.to_string() }
}

pub fn get(addr: SocketAddr, path: &str) -> Answer {
    request(addr, "GET", path, None)
}

/// Sends `body` as JSON.
pub fn send(addr: SocketAddr, method: &str, path: &str, body: &Value) -> Answer {
    request(addr, method, path, Some(("application/json", &body.to_string())))
}
