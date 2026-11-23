//! The demo app over real HTTP, on a loopback port picked by the system.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};

use serde_json::Value;
use tileboard::contract::{Catalogue, Data};
use tileboard::demo;

/// Starts the demo app on a free loopback port and returns its address.
fn start() -> SocketAddr {
    // Bound before the server thread starts, so early requests wait instead of failing.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .build()
            .unwrap();
        runtime.block_on(async move {
            let listener = tokio::net::TcpListener::from_std(listener).unwrap();
            demo::serve(listener).await.unwrap();
        });
    });
    addr
}

struct Answer {
    status: u16,
    head: String,
    body: String,
}

impl Answer {
    fn json(&self) -> Value {
        assert!(
            self.head.contains("content-type: application/json"),
            "not JSON: {}",
            self.head
        );
        serde_json::from_str(&self.body).unwrap_or_else(|e| panic!("body is not JSON: {e}\n{}", self.body))
    }
}

/// One plain HTTP/1.1 request. `Connection: close` lets us read until the end.
fn request(addr: SocketAddr, method: &str, path: &str) -> Answer {
    let mut stream = TcpStream::connect(addr).unwrap();
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let mut raw = String::new();
    stream.read_to_string(&mut raw).unwrap();

    let (head, body) = raw
        .split_once("\r\n\r\n")
        .unwrap_or_else(|| panic!("no header end in {raw:?}"));
    let head = head.to_ascii_lowercase();
    assert!(
        !head.contains("transfer-encoding"),
        "this small client cannot read chunked answers"
    );
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|code| code.parse().ok())
        .expect("a status code");
    Answer {
        status,
        head,
        body: body.to_string(),
    }
}

fn get(addr: SocketAddr, path: &str) -> Answer {
    request(addr, "GET", path)
}

#[test]
fn the_catalogue_is_served() {
    let addr = start();
    let answer = get(addr, "/widgets");
    assert_eq!(answer.status, 200);
    let catalogue: Catalogue = serde_json::from_value(answer.json()).unwrap();
    assert_eq!(&catalogue, demo::catalogue());
}

#[test]
fn every_source_is_served_with_no_query_string() {
    let addr = start();
    for source in &demo::catalogue().sources {
        let answer = get(addr, &format!("/widgets/{}", source.id));
        assert_eq!(answer.status, 200, "{}", source.id);
        let data: Data = serde_json::from_value(answer.json()).unwrap();
        assert_eq!(source.data_problems(&data), Vec::<String>::new(), "{}", source.id);
    }
}

#[test]
fn params_come_from_the_query_string() {
    let addr = start();
    let answer = get(addr, "/widgets/server?host=comet&points=7&colour=red");
    assert_eq!(answer.status, 200);
    assert_eq!(
        answer.json()["values"]["cpu_history"]["series"]
            .as_array()
            .unwrap()
            .len(),
        7
    );
}

#[test]
fn an_unknown_source_is_404() {
    let addr = start();
    let answer = get(addr, "/widgets/nope");
    assert_eq!(answer.status, 404);
    assert_eq!(answer.json()["error"], "there is no source \"nope\"");
}

#[test]
fn a_bad_param_value_is_400() {
    let addr = start();
    for path in [
        "/widgets/server?host=zzz",
        "/widgets/server?points=3",
        "/widgets/spending?period=year",
    ] {
        let answer = get(addr, path);
        assert_eq!(answer.status, 400, "{path}");
        assert!(
            answer.json()["error"]
                .as_str()
                .unwrap()
                .contains("is not a valid value"),
            "{path}"
        );
    }
}

#[test]
fn other_paths_are_404() {
    let addr = start();
    for path in ["/nope", "/widgets/", "/widgets/server/extra", "/widget"] {
        let answer = get(addr, path);
        assert_eq!(answer.status, 404, "{path}");
        assert!(answer.json()["error"].is_string(), "{path}");
    }
}

#[test]
fn only_get_is_allowed() {
    let addr = start();
    for method in ["POST", "PUT", "PATCH", "DELETE"] {
        for path in ["/widgets", "/widgets/server"] {
            assert_eq!(request(addr, method, path).status, 405, "{method} {path}");
        }
    }
}

#[test]
fn people_get_a_page_at_the_root() {
    let addr = start();
    let answer = get(addr, "/");
    assert_eq!(answer.status, 200);
    assert!(answer.head.contains("content-type: text/html"));
    assert!(answer.body.contains("Everything here is made up"));
}
