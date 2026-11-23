//! The dashboard server over real HTTP: against the demo app, and against a
//! fake app that can be slow, down, wrong or huge on request.

mod common;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::{Value, json};

use common::{get as http_get, request, send};
use tileboard::config::Config;
use tileboard::contract::Catalogue;
use tileboard::dashboard::{self, Dashboard, MAX_SOURCES};
use tileboard::hub::{Hub, TIMEOUT};
use tileboard::layout::{Layout, LayoutStore};
use tileboard::web::Web;
use tileboard::{demo, rfc3339};

// ------------------------------------------------------------------ helpers

/// Starts a dashboard for the apps in `config` (TOML text) and returns its address.
fn dashboard_with(config: &str, starter: Layout, timeout: Duration) -> SocketAddr {
    let config = Config::parse(config, Path::new("")).unwrap();
    let state = Arc::new(Dashboard {
        hub: Hub::with_timeout(config.apps, timeout),
        layout: LayoutStore::open(None, starter).unwrap(),
        web: Web::built_in(),
    });
    common::start(move |listener| dashboard::serve(listener, state))
}

fn app_block(name: &str, addr: SocketAddr) -> String {
    format!("[[apps]]\nname = \"{name}\"\nbase_url = \"http://{addr}\"\n")
}

/// A dashboard that shows one app under the name `app`.
fn dashboard_for(app: SocketAddr) -> SocketAddr {
    dashboard_with(&app_block("app", app), Layout::default(), TIMEOUT)
}

/// The demo app, and a dashboard that shows it as "demo" with the demo starter layout.
fn demo_dashboard() -> (SocketAddr, SocketAddr) {
    let app = common::start(demo::serve);
    (
        dashboard_with(&app_block("demo", app), demo::starter_layout("demo"), TIMEOUT),
        app,
    )
}

/// `POST /api/data` for `sources`; the whole answer.
fn data(dashboard: SocketAddr, sources: Value) -> Value {
    send(dashboard, "POST", "/api/data", &json!({ "sources": sources })).ok_json()
}

/// `POST /api/data` for one source; that source's answer.
fn one(dashboard: SocketAddr, source: Value) -> Value {
    data(dashboard, json!([source]))["sources"][0].clone()
}

/// The first app of `GET /api/apps`.
fn first_app(dashboard: SocketAddr, path: &str) -> Value {
    http_get(dashboard, path).ok_json()["apps"][0].clone()
}

fn error_of(answer: &Value) -> &str {
    answer["error"]
        .as_str()
        .unwrap_or_else(|| panic!("no error in {answer}"))
}

/// Longer than the fake app's refresh (1 s), the retry wait and the forced-refresh floor.
fn wait_out_a_refresh() {
    std::thread::sleep(Duration::from_millis(1150));
}

// ----------------------------------------------------------------- fake app

#[derive(Clone, Copy)]
enum Mode {
    Good,
    Error500,
    Slow,
    Huge,
    BadFields,
    Redirect,
    NotJson,
    BadDate,
}

/// An app whose behaviour the tests switch. Its one source `s` refreshes every second;
/// field `n` counts the calls and field `t` echoes the `who` param.
struct Fake {
    mode: Mutex<Mode>,
    /// What `/widgets` answers. `null` makes it answer 500.
    catalogue: Mutex<Value>,
    /// The `link` of the data answer.
    link: Mutex<Value>,
    token: Option<&'static str>,
    data_calls: AtomicUsize,
}

fn fake_catalogue() -> Value {
    json!({
        "contract": 2,
        "app": "fake",
        "title": "Fake app",
        "sources": [{
            "id": "s",
            "title": "Source",
            "refresh_s": 1,
            "params": [{"name": "who", "type": "text", "default": "a b"}],
            "fields": [
                {"id": "n", "label": "Calls", "type": "number", "warn": 5, "crit": 10},
                {"id": "t", "label": "Who", "type": "text"},
                {"id": "st", "label": "State", "type": "status"},
                {"id": "se", "label": "Series", "type": "series"},
                {"id": "l", "label": "List", "type": "list"}
            ]
        }]
    })
}

impl Fake {
    fn start() -> (Arc<Fake>, SocketAddr) {
        Fake::start_with(fake_catalogue(), None)
    }

    fn start_with(catalogue: Value, token: Option<&'static str>) -> (Arc<Fake>, SocketAddr) {
        let fake = Arc::new(Fake {
            mode: Mutex::new(Mode::Good),
            catalogue: Mutex::new(catalogue),
            link: Mutex::new(json!("/s?x=1")),
            token,
            data_calls: AtomicUsize::new(0),
        });
        let router = Router::new()
            .route("/widgets", get(fake_widgets))
            .route("/widgets/s", get(fake_data))
            .with_state(fake.clone());
        let addr = common::start(move |listener| async move { axum::serve(listener, router).await });
        (fake, addr)
    }

    fn set(&self, mode: Mode) {
        *self.mode.lock().unwrap() = mode;
    }

    fn calls(&self) -> usize {
        self.data_calls.load(Ordering::SeqCst)
    }

    fn lets_in(&self, headers: &HeaderMap) -> bool {
        match self.token {
            None => true,
            Some(token) => headers
                .get(header::AUTHORIZATION)
                .is_some_and(|sent| sent == format!("Bearer {token}").as_str()),
        }
    }
}

fn no_token() -> Response {
    (StatusCode::UNAUTHORIZED, Json(json!({"error": "no token"}))).into_response()
}

async fn fake_widgets(State(fake): State<Arc<Fake>>, headers: HeaderMap) -> Response {
    if !fake.lets_in(&headers) {
        return no_token();
    }
    let catalogue = fake.catalogue.lock().unwrap().clone();
    if catalogue.is_null() {
        StatusCode::INTERNAL_SERVER_ERROR.into_response()
    } else {
        Json(catalogue).into_response()
    }
}

async fn fake_data(
    State(fake): State<Arc<Fake>>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let call = fake.data_calls.fetch_add(1, Ordering::SeqCst) + 1;
    if !fake.lets_in(&headers) {
        return no_token();
    }
    let mode = *fake.mode.lock().unwrap();
    let mut good = json!({
        "updated_at": "2026-11-24T00:00:00Z",
        "link": fake.link.lock().unwrap().clone(),
        "values": {
            "n": {"value": call},
            "t": {"value": query.get("who")},
            "st": {"value": "crit"},
            "se": {"value": 2, "series": [1, null, 2]},
            "l": {"value": [{"label": "a", "value": 1}]}
        }
    });
    match mode {
        Mode::Good => Json(good).into_response(),
        Mode::Error500 => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "the database is asleep"})),
        )
            .into_response(),
        Mode::Slow => {
            tokio::time::sleep(Duration::from_secs(3)).await;
            Json(good).into_response()
        }
        Mode::Huge => "x".repeat(2 * 1024 * 1024).into_response(),
        Mode::BadFields => {
            good["values"]["n"] = json!({"value": "seven"});
            good["values"]["t"] = json!("just text");
            good["values"]["l"] = json!({"value": 1, "series": [1]});
            good["values"].as_object_mut().unwrap().remove("st");
            good["values"]["extra"] = json!({"value": 1});
            Json(good).into_response()
        }
        Mode::Redirect => (StatusCode::FOUND, [(header::LOCATION, "/widgets/s?who=redirected")]).into_response(),
        Mode::NotJson => "<html>hello</html>".into_response(),
        Mode::BadDate => {
            good["updated_at"] = json!("yesterday");
            Json(good).into_response()
        }
    }
}

/// A fake app and a dashboard that shows it as "app".
fn fake_dashboard() -> (Arc<Fake>, SocketAddr) {
    let (fake, app) = Fake::start();
    (fake, dashboard_for(app))
}

/// The source of the fake app, asked with `who`. A new `who` is a new answer slot, so it is always a fresh call.
fn ask(dashboard: SocketAddr, who: &str) -> Value {
    one(dashboard, json!({"app": "app", "source": "s", "params": {"who": who}}))
}

// ------------------------------------------------- against the demo app

#[test]
fn the_apps_endpoint_gives_the_demo_catalogue() {
    let (dashboard, _) = demo_dashboard();
    let app = first_app(dashboard, "/api/apps");
    assert_eq!(app["name"], "demo");
    assert_eq!(app["ok"], true);
    assert_eq!(app["error"], Value::Null);
    assert!(rfc3339::parse(app["checked_at"].as_str().unwrap()).is_some());
    let catalogue: Catalogue = serde_json::from_value(app["catalogue"].clone()).unwrap();
    assert_eq!(&catalogue, demo::catalogue());
}

#[test]
fn demo_data_comes_checked_with_statuses_and_full_links() {
    let (dashboard, app) = demo_dashboard();
    let answer = data(
        dashboard,
        json!([
            {"app": "demo", "source": "server"},
            {"app": "demo", "source": "server", "params": {"host": "comet", "points": "5"}},
            {"app": "demo", "source": "weather", "params": {"place": "emberfall"}},
            {"app": "demo", "source": "spending", "params": {"period": "month"}}
        ]),
    );
    assert!(rfc3339::parse(answer["now"].as_str().unwrap()).is_some());
    assert_eq!(answer["layout_rev"], 0);
    let sources = answer["sources"].as_array().unwrap();
    assert_eq!(sources.len(), 4);

    for (answer, source) in sources.iter().zip(["server", "server", "weather", "spending"]) {
        assert_eq!(answer["ok"], true, "{answer}");
        assert_eq!(answer["error"], Value::Null);
        assert_eq!((&answer["app"], &answer["source"]), (&json!("demo"), &json!(source)));
        let spec = demo::catalogue().source(source).unwrap();
        assert_eq!(answer["refresh_s"], spec.refresh_s);
        assert_eq!(answer["link"], format!("http://{app}/#{source}"));
        assert!(rfc3339::parse(answer["updated_at"].as_str().unwrap()).is_some());
        assert!(rfc3339::parse(answer["fetched_at"].as_str().unwrap()).is_some());

        // Exactly the catalogue's fields, none of them in error, and a status wherever one can be worked out.
        let values = answer["values"].as_object().unwrap();
        assert_eq!(values.len(), spec.fields.len());
        for field in &spec.fields {
            let entry = &values[&field.id];
            assert_eq!(entry.get("error"), None, "{source}.{}", field.id);
            let has_rule = field.warn.is_some() || field.kind == tileboard::contract::FieldType::Status;
            assert_eq!(
                entry.get("status").is_some(),
                has_rule,
                "{source}.{}: {entry}",
                field.id
            );
        }
    }

    // Left-out params are filled in with their defaults.
    assert_eq!(sources[0]["params"], json!({"host": "atlas", "points": "30"}));
    assert_eq!(sources[1]["params"], json!({"host": "comet", "points": "5"}));
    assert_eq!(
        sources[1]["values"]["cpu_history"]["series"].as_array().unwrap().len(),
        5
    );
    assert_eq!(sources[3]["params"], json!({"period": "month"}));
}

#[test]
fn one_request_can_mix_good_and_bad_sources() {
    let (dashboard, _) = demo_dashboard();
    let answer = data(
        dashboard,
        json!([
            {"app": "demo", "source": "weather"},
            {"app": "nope", "source": "weather"},
            {"app": "demo", "source": "nope"},
            {"app": "demo", "source": "server", "params": {"host": "zzz"}},
            {"app": "demo", "source": "server", "params": {"points": "many"}},
            {"app": "demo", "source": "spending", "params": {"colour": "red"}}
        ]),
    );
    let sources = answer["sources"].as_array().unwrap();
    assert_eq!(sources[0]["ok"], true);
    assert_eq!(error_of(&sources[1]), "there is no app \"nope\" in the config");
    assert_eq!(error_of(&sources[2]), "the app has no source \"nope\"");
    assert_eq!(error_of(&sources[3]), "\"zzz\" is not a valid value for \"host\"");
    assert_eq!(error_of(&sources[4]), "\"many\" is not a valid value for \"points\"");
    for bad in &sources[1..5] {
        assert_eq!((&bad["ok"], &bad["values"]), (&json!(false), &json!({})), "{bad}");
    }
    // A param the source does not have is dropped, not an error.
    assert_eq!(sources[5]["ok"], true);
    assert_eq!(sources[5]["params"], json!({"period": "week"}));
}

#[test]
fn an_app_that_is_not_there_does_not_hide_the_others() {
    let demo_app = common::start(demo::serve);
    let config = format!(
        "{}{}",
        app_block("gone", common::closed_port()),
        app_block("demo", demo_app)
    );
    let dashboard = dashboard_with(&config, Layout::default(), TIMEOUT);

    let apps = http_get(dashboard, "/api/apps").ok_json()["apps"].clone();
    assert_eq!(
        (&apps[0]["name"], &apps[0]["ok"], &apps[0]["catalogue"]),
        (&json!("gone"), &json!(false), &Value::Null)
    );
    assert!(error_of(&apps[0]).starts_with("cannot reach the app ("), "{}", apps[0]);
    assert_eq!((&apps[1]["name"], &apps[1]["ok"]), (&json!("demo"), &json!(true)));

    let answer = data(
        dashboard,
        json!([{"app": "gone", "source": "x"}, {"app": "demo", "source": "weather"}]),
    );
    assert!(error_of(&answer["sources"][0]).starts_with("cannot reach the app ("));
    assert_eq!(answer["sources"][1]["ok"], true);
}

// ------------------------------------------------- caching, against the fake

#[test]
fn a_source_is_called_once_per_refresh_however_often_it_is_asked() {
    let (fake, dashboard) = fake_dashboard();

    let first = ask(dashboard, "x");
    assert_eq!(first["values"]["n"], json!({"value": 1, "status": "ok"}));
    for _ in 0..5 {
        assert_eq!(ask(dashboard, "x")["values"]["n"]["value"], 1);
    }
    assert_eq!(fake.calls(), 1);

    wait_out_a_refresh();
    assert_eq!(ask(dashboard, "x")["values"]["n"]["value"], 2);
    assert_eq!(fake.calls(), 2);
}

#[test]
fn the_same_source_asked_many_times_at_once_is_one_call() {
    let (fake, dashboard) = fake_dashboard();
    let same: Vec<Value> = (0..10)
        .map(|_| json!({"app": "app", "source": "s", "params": {"who": "x"}}))
        .collect();
    let answer = data(dashboard, json!(same));
    assert_eq!(answer["sources"].as_array().unwrap().len(), 10);
    assert!(
        answer["sources"]
            .as_array()
            .unwrap()
            .iter()
            .all(|source| source["values"]["n"]["value"] == 1)
    );
    assert_eq!(fake.calls(), 1);
}

#[test]
fn a_default_given_or_left_out_is_the_same_answer() {
    let (fake, dashboard) = fake_dashboard();
    let left_out = one(dashboard, json!({"app": "app", "source": "s"}));
    let given = ask(dashboard, "a b");
    let with_extra = one(
        dashboard,
        json!({"app": "app", "source": "s", "params": {"other": "1"}}),
    );
    for answer in [&left_out, &given, &with_extra] {
        assert_eq!(answer["params"], json!({"who": "a b"}));
        assert_eq!(answer["values"]["t"]["value"], "a b");
    }
    assert_eq!(fake.calls(), 1);

    // Another value is another answer.
    assert_eq!(ask(dashboard, "someone else")["values"]["n"]["value"], 2);
}

#[test]
fn param_values_reach_the_app_unharmed() {
    let (_, dashboard) = fake_dashboard();
    for who in ["two words", "a&b=c", "100%", "é and ü", "x/y?z#w+v", ""] {
        assert_eq!(ask(dashboard, who)["values"]["t"]["value"], who, "{who:?}");
    }
}

// ------------------------------------------------- when things go wrong

#[test]
fn a_failed_call_keeps_the_last_good_values() {
    let (fake, dashboard) = fake_dashboard();
    let good = ask(dashboard, "x");
    assert_eq!((&good["ok"], &good["values"]["n"]["value"]), (&json!(true), &json!(1)));

    fake.set(Mode::Error500);
    wait_out_a_refresh();
    let failed = ask(dashboard, "x");
    assert_eq!(failed["ok"], false);
    assert_eq!(error_of(&failed), "the app answered 500: the database is asleep");
    assert_eq!(failed["values"], good["values"], "the last good values are still there");
    assert_eq!(failed["fetched_at"], good["fetched_at"]);

    fake.set(Mode::Good);
    wait_out_a_refresh();
    let back = ask(dashboard, "x");
    assert_eq!((&back["ok"], &back["error"]), (&json!(true), &Value::Null));
    assert_eq!(back["values"]["n"]["value"], 3);
}

#[test]
fn a_call_that_never_worked_has_no_values() {
    let (fake, dashboard) = fake_dashboard();
    fake.set(Mode::Error500);
    let failed = ask(dashboard, "x");
    assert_eq!((&failed["ok"], &failed["values"]), (&json!(false), &json!({})));
    assert_eq!(
        (&failed["fetched_at"], &failed["updated_at"], &failed["link"]),
        (&Value::Null, &Value::Null, &Value::Null)
    );
    assert_eq!(failed["refresh_s"], 1);
}

#[test]
fn a_slow_app_is_given_up_on() {
    let (fake, app) = Fake::start();
    let dashboard = dashboard_with(&app_block("app", app), Layout::default(), Duration::from_millis(400));
    assert_eq!(ask(dashboard, "warm-up")["ok"], true);

    fake.set(Mode::Slow);
    let started = std::time::Instant::now();
    let answer = ask(dashboard, "x");
    assert_eq!(error_of(&answer), "the app did not answer within 0.4 s");
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "took {:?}",
        started.elapsed()
    );
}

#[test]
fn bad_answers_are_refused_with_a_reason() {
    let (fake, dashboard) = fake_dashboard();
    let cases = [
        (Mode::Huge, "the app's answer is larger than 1 MB"),
        (Mode::Redirect, "the app answered 302"),
        (Mode::NotJson, "the app's answer does not follow the spec: "),
        (
            Mode::BadDate,
            "the app's answer does not follow the spec: updated_at is not a date-time",
        ),
    ];
    for (i, (mode, expected)) in cases.into_iter().enumerate() {
        fake.set(mode);
        let answer = ask(dashboard, &format!("case-{i}"));
        assert_eq!(answer["ok"], false);
        assert!(
            error_of(&answer).starts_with(expected),
            "expected {expected:?}, got {answer}"
        );
    }
    // The redirect was not followed: one call per case, no more.
    assert_eq!(fake.calls(), 4);
}

#[test]
fn one_bad_field_does_not_take_the_others_down() {
    let (fake, dashboard) = fake_dashboard();
    fake.set(Mode::BadFields);
    let answer = ask(dashboard, "x");
    assert_eq!((&answer["ok"], &answer["error"]), (&json!(true), &Value::Null));
    assert_eq!(
        answer["values"],
        json!({
            "n": {"value": null, "error": "value does not fit a number field"},
            "t": {"value": null, "error": "the app sent something that is not an entry"},
            "st": {"value": null},
            "se": {"value": 2, "series": [1, null, 2]},
            "l": {"value": null, "error": "value does not fit a list field"}
        })
    );
}

#[test]
fn links_become_full_addresses_and_odd_ones_are_dropped() {
    let (fake, app) = Fake::start();
    let dashboard = dashboard_for(app);
    assert_eq!(ask(dashboard, "path")["link"], format!("http://{app}/s?x=1"));

    *fake.link.lock().unwrap() = json!("https://elsewhere.example/page");
    assert_eq!(ask(dashboard, "full")["link"], "https://elsewhere.example/page");

    for (i, odd) in [
        json!("javascript:alert(1)"),
        json!("//other.example/x"),
        json!("page"),
        json!(5),
        json!(null),
    ]
    .into_iter()
    .enumerate()
    {
        *fake.link.lock().unwrap() = odd.clone();
        let answer = ask(dashboard, &format!("odd-{i}"));
        assert_eq!((&answer["ok"], &answer["link"]), (&json!(true), &Value::Null), "{odd}");
    }

    // With a public_url, paths are put behind that instead of the address the server calls.
    *fake.link.lock().unwrap() = json!("/s?x=1");
    let config = format!("{}public_url = \"https://fake.example/app/\"\n", app_block("app", app));
    let dashboard = dashboard_with(&config, Layout::default(), TIMEOUT);
    assert_eq!(ask(dashboard, "public")["link"], "https://fake.example/app/s?x=1");
}

#[test]
fn the_token_is_sent_and_never_shown() {
    let (_, app) = Fake::start_with(fake_catalogue(), Some("s3cr3t-token"));
    let with = |token_line: &str| {
        dashboard_with(
            &format!("{}{token_line}", app_block("app", app)),
            Layout::default(),
            TIMEOUT,
        )
    };

    let right = with("token = \"s3cr3t-token\"\n");
    let apps = http_get(right, "/api/apps");
    assert_eq!(apps.ok_json()["apps"][0]["ok"], true);
    assert!(!apps.body.contains("s3cr3t"), "the token leaked: {}", apps.body);
    let answer = send(
        right,
        "POST",
        "/api/data",
        &json!({"sources": [{"app": "app", "source": "s"}]}),
    );
    assert_eq!(answer.ok_json()["sources"][0]["ok"], true);
    assert!(!answer.body.contains("s3cr3t"), "the token leaked: {}", answer.body);

    for wrong in [with("token = \"not-the-token\"\n"), with("")] {
        let app = first_app(wrong, "/api/apps");
        assert_eq!(app["ok"], false);
        assert_eq!(error_of(&app), "the app answered 401: the token is missing or wrong");
        assert_eq!(
            error_of(&ask(wrong, "x")),
            "the app answered 401: the token is missing or wrong"
        );
    }
}

// ------------------------------------------------- catalogues

#[test]
fn a_catalogue_that_does_not_follow_the_spec_is_refused() {
    let mut twice = fake_catalogue();
    let field = twice["sources"][0]["fields"][0].clone();
    twice["sources"][0]["fields"].as_array_mut().unwrap().push(field);
    let mut wrong_shape = fake_catalogue();
    wrong_shape["sources"][0]["refresh_s"] = json!("often");
    let mut v1 = fake_catalogue();
    v1["contract"] = json!(1);

    let cases = [
        (v1, "the app speaks widget contract 1; this dashboard speaks contract 2"),
        (
            json!({"hello": "world"}),
            "the app's /widgets answer is not a contract 2 catalogue",
        ),
        (
            json!("just text"),
            "the app's /widgets answer is not a contract 2 catalogue",
        ),
        (
            twice,
            "the app's catalogue does not follow the spec: source \"s\", field \"n\": id is used twice",
        ),
        (wrong_shape, "the app's catalogue does not follow the spec: "),
    ];
    for (catalogue, expected) in cases {
        let (_, app) = Fake::start_with(catalogue, None);
        let dashboard = dashboard_for(app);
        let app = first_app(dashboard, "/api/apps");
        assert_eq!((&app["ok"], &app["catalogue"]), (&json!(false), &Value::Null));
        assert!(error_of(&app).starts_with(expected), "expected {expected:?}, got {app}");
        assert!(error_of(&ask(dashboard, "x")).starts_with(expected));
    }
}

#[test]
fn the_last_good_catalogue_is_kept_when_the_app_goes_away() {
    let (fake, dashboard) = fake_dashboard();
    let before = first_app(dashboard, "/api/apps");
    assert_eq!(before["ok"], true);

    // The app's /widgets now fails. Without a refresh the kept catalogue is still good for 5 minutes.
    *fake.catalogue.lock().unwrap() = Value::Null;
    assert_eq!(first_app(dashboard, "/api/apps")["ok"], true);

    wait_out_a_refresh();
    let after = first_app(dashboard, "/api/apps?refresh=1");
    assert_eq!(after["ok"], false);
    assert_eq!(error_of(&after), "the app answered 500");
    assert_eq!(
        after["catalogue"], before["catalogue"],
        "the last good catalogue is still there"
    );

    // Its sources can still be asked, with that catalogue.
    assert_eq!(ask(dashboard, "x")["ok"], true);

    // And it comes back.
    *fake.catalogue.lock().unwrap() = fake_catalogue();
    wait_out_a_refresh();
    assert_eq!(first_app(dashboard, "/api/apps?refresh=1")["ok"], true);
}

// ------------------------------------------------- layout

#[test]
fn the_layout_can_be_read_and_saved() {
    let (dashboard, _) = demo_dashboard();
    let mut layout = http_get(dashboard, "/api/layout").ok_json();
    assert_eq!(layout, serde_json::to_value(demo::starter_layout("demo")).unwrap());

    // Move a tile and save.
    layout["tiles"][0]["x"] = json!(9);
    let saved = send(dashboard, "PUT", "/api/layout", &layout).ok_json();
    assert_eq!(saved["rev"], 1);
    assert_eq!(saved["tiles"], layout["tiles"]);
    assert_eq!(http_get(dashboard, "/api/layout").ok_json(), saved);

    // A data answer tells open pages that the layout changed.
    assert_eq!(data(dashboard, json!([]))["layout_rev"], 1);
}

#[test]
fn a_save_from_an_old_layout_is_a_conflict() {
    let (dashboard, _) = demo_dashboard();
    let layout = http_get(dashboard, "/api/layout").ok_json();
    send(dashboard, "PUT", "/api/layout", &layout).ok_json();

    // Someone still holding rev 0 saves again.
    let answer = send(dashboard, "PUT", "/api/layout", &layout);
    assert_eq!(answer.status, 409);
    assert_eq!(
        answer.json(),
        json!({"error": "the layout was changed somewhere else; load it again", "rev": 1})
    );
}

#[test]
fn a_layout_that_is_not_valid_is_not_saved() {
    let (dashboard, _) = demo_dashboard();
    let good = http_get(dashboard, "/api/layout").ok_json();

    let mut too_wide = good.clone();
    too_wide["tiles"][0]["w"] = json!(13);
    let answer = send(dashboard, "PUT", "/api/layout", &too_wide);
    assert_eq!(answer.status, 400);
    assert_eq!(answer.json()["error"], "the layout is not valid");
    assert_eq!(
        answer.json()["problems"],
        json!(["tile 1: x + w goes past the 12 columns"])
    );

    let mut unknown_property = good.clone();
    unknown_property["tiles"][0]["colour"] = json!("red");
    let mut unknown_view = good.clone();
    unknown_view["tiles"][0]["view"] = json!("gauge");
    for bad in [unknown_property, unknown_view, json!({"rev": 0}), json!([])] {
        let answer = send(dashboard, "PUT", "/api/layout", &bad);
        assert_eq!(answer.status, 422, "{bad}");
        assert!(answer.json()["error"].is_string());
    }

    let not_json = request(dashboard, "PUT", "/api/layout", Some(("application/json", "{ nope")));
    assert_eq!(not_json.status, 400);
    let not_marked_json = request(dashboard, "PUT", "/api/layout", Some(("text/plain", &good.to_string())));
    assert_eq!(not_marked_json.status, 415);

    assert_eq!(http_get(dashboard, "/api/layout").ok_json(), good, "nothing was saved");
}

// ------------------------------------------------- the rest of the API

#[test]
fn a_data_request_has_limits() {
    let (dashboard, _) = demo_dashboard();
    let too_many: Vec<Value> = (0..=MAX_SOURCES)
        .map(|_| json!({"app": "demo", "source": "weather"}))
        .collect();
    let answer = send(dashboard, "POST", "/api/data", &json!({ "sources": too_many }));
    assert_eq!(answer.status, 400);
    assert_eq!(answer.json()["error"], "one request can name at most 200 sources");

    for bad in [
        json!({}),
        json!({"sources": [{"app": "demo"}]}),
        json!({"sources": [], "extra": 1}),
    ] {
        let answer = send(dashboard, "POST", "/api/data", &bad);
        assert_eq!(answer.status, 422, "{bad}");
        assert!(answer.json()["error"].is_string());
    }
    assert_eq!(data(dashboard, json!([]))["sources"], json!([]));
    assert_eq!(http_get(dashboard, "/api/data").status, 405);
}

#[test]
fn the_frontend_is_served_locked_down() {
    let (dashboard, _) = demo_dashboard();

    let page = http_get(dashboard, "/");
    assert_eq!(page.status, 200);
    assert!(page.head.contains("content-type: text/html; charset=utf-8"));
    assert!(
        page.head
            .contains("content-security-policy: default-src 'none'; script-src 'self';")
    );
    assert!(page.head.contains("frame-ancestors 'none'"));
    assert!(page.head.contains("x-content-type-options: nosniff"));
    assert!(page.head.contains("referrer-policy: no-referrer"));
    assert!(page.body.contains("<title>tileboard</title>"));

    let script = http_get(dashboard, "/app.js");
    assert_eq!(script.status, 200);
    assert!(script.head.contains("content-type: text/javascript"));
    let style = http_get(dashboard, "/style.css");
    assert_eq!(style.status, 200);
    assert!(style.head.contains("content-type: text/css"));

    for path in ["/nope", "/app.js.map", "/api/nope", "/../Cargo.toml", "/web/index.html"] {
        let answer = http_get(dashboard, path);
        assert_eq!(answer.status, 404, "{path}");
        assert!(answer.json()["error"].is_string());
    }
    assert_eq!(request(dashboard, "POST", "/", None).status, 405);
}

#[test]
fn api_answers_are_not_kept_by_browsers() {
    let (dashboard, _) = demo_dashboard();
    for answer in [http_get(dashboard, "/api/apps"), http_get(dashboard, "/api/layout")] {
        assert!(answer.head.contains("cache-control: no-store"), "{}", answer.head);
        assert!(answer.head.contains("x-content-type-options: nosniff"));
    }
}
