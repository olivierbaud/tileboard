//! The demo app over real HTTP, on a loopback port picked by the system.

mod common;

use std::net::SocketAddr;

use common::{get, request};
use tileboard::contract::{Catalogue, Data};
use tileboard::demo;

fn start() -> SocketAddr {
    common::start(demo::serve)
}

#[test]
fn the_catalogue_is_served() {
    let addr = start();
    let catalogue: Catalogue = serde_json::from_value(get(addr, "/widgets").ok_json()).unwrap();
    assert_eq!(&catalogue, demo::catalogue());
}

#[test]
fn every_source_is_served_with_no_query_string() {
    let addr = start();
    for source in &demo::catalogue().sources {
        let answer = get(addr, &format!("/widgets/{}", source.id));
        let data: Data = serde_json::from_value(answer.ok_json()).unwrap();
        assert_eq!(source.data_problems(&data), Vec::<String>::new(), "{}", source.id);
    }
}

#[test]
fn params_come_from_the_query_string() {
    let addr = start();
    let answer = get(addr, "/widgets/server?host=comet&points=7&colour=red").ok_json();
    assert_eq!(answer["values"]["cpu_history"]["series"].as_array().unwrap().len(), 7);
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
            assert_eq!(request(addr, method, path, None).status, 405, "{method} {path}");
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
