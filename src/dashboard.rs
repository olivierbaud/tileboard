//! The dashboard server: the browser's API and the frontend files.
//!
//! | Address | What |
//! |---|---|
//! | `GET /api/apps` | every configured app with its catalogue (`?refresh=1` reads them again) |
//! | `POST /api/data` | the data of the sources the browser names, cached per refresh |
//! | `GET /api/layout` | the saved layout |
//! | `PUT /api/layout` | save the whole layout (must carry the `rev` it started from) |
//! | `GET /`, `/app.js`, `/style.css` | the frontend |
//! | `GET /connect.md` | everything a developer or an agent needs to connect an app, as one text |
//! | `GET /schema/*.schema.json` | the contract's JSON Schemas |
//! | `POST /api/check` | call an app the way the dashboard would and say what is wrong (if `manage_apps`) |
//! | `POST /api/apps`, `DELETE /api/apps/<name>` | add or remove an app (if `manage_apps`) |

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use axum::extract::rejection::JsonRejection;
use axum::extract::{Path, Query, State};
use axum::http::header::{
    CACHE_CONTROL, CONTENT_SECURITY_POLICY, CONTENT_TYPE, HOST, REFERRER_POLICY, X_CONTENT_TYPE_OPTIONS,
};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::config::{AppConfig, AppEntry, save_managed_apps};
use crate::hub::{AppChangeError, AppStatus, CheckReport, Hub, SourceAnswer, SourceRequest};
use crate::layout::{Layout, LayoutStore, SaveError};
use crate::rfc3339;
use crate::web::{FILES, Web, WebFile};

/// The most sources one data request may name.
pub const MAX_SOURCES: usize = 200;

/// The page may only load its own files: no inline script, nothing from another site, no framing.
const PAGE_POLICY: &str = "default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' data:; \
                           connect-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'";

/// What an agent or a developer reads to connect an app. The contract itself is added after it.
const CONNECT_KIT: &str = include_str!("connect.md");
const SPEC: &str = include_str!("../SPEC.md");
const CATALOGUE_SCHEMA: &str = include_str!("../schema/catalogue.schema.json");
const DATA_SCHEMA: &str = include_str!("../schema/data.schema.json");

const MANAGE_OFF: &str = "this dashboard does not let apps be checked, added or removed from outside; \
                          its owner can allow it with `manage_apps = true` in config.toml";

pub struct Dashboard {
    pub hub: Hub,
    pub layout: LayoutStore,
    pub web: Web,
    /// Whether apps may be checked, added and removed through the API (and so from the settings page).
    manage_apps: bool,
    /// Where apps added that way are kept. `None`: in memory only.
    apps_file: Option<PathBuf>,
    /// Held while the apps file is written, so writes go one at a time.
    saving_apps: Mutex<()>,
}

impl Dashboard {
    /// A dashboard whose apps are the ones it was started with, and nobody can change them from outside.
    pub fn new(hub: Hub, layout: LayoutStore, web: Web) -> Dashboard {
        Dashboard { hub, layout, web, manage_apps: false, apps_file: None, saving_apps: Mutex::new(()) }
    }

    /// Lets apps be checked, added and removed through the API. Added apps are kept in `apps_file`, if there is one.
    pub fn managing_apps(mut self, apps_file: Option<PathBuf>) -> Dashboard {
        self.manage_apps = true;
        self.apps_file = apps_file;
        self
    }

    /// Writes the apps added from the settings page to their file.
    fn save_apps(&self) -> std::io::Result<()> {
        let Some(path) = &self.apps_file else { return Ok(()) };
        let _one_at_a_time = self.saving_apps.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        save_managed_apps(path, &self.hub.configs())
    }
}

pub fn router(dashboard: Arc<Dashboard>) -> Router {
    let mut router = Router::new()
        .route("/api/apps", get(get_apps).post(post_app))
        .route("/api/apps/{name}", delete(delete_app))
        .route("/api/check", post(post_check))
        .route("/api/data", post(post_data))
        .route("/api/layout", get(get_layout).put(put_layout))
        .route("/connect.md", get(connect_kit))
        .route("/schema/catalogue.schema.json", get(|| async { schema(CATALOGUE_SCHEMA) }))
        .route("/schema/data.schema.json", get(|| async { schema(DATA_SCHEMA) }));
    for file in FILES {
        router = router.route(
            file.route,
            get(move |State(dashboard): State<Arc<Dashboard>>| async move { web_file(&dashboard.web, file) }),
        );
    }
    router.fallback(|| async { error(StatusCode::NOT_FOUND, "there is nothing at this address") }).with_state(dashboard)
}

/// Serves the dashboard on `listener` until the process stops.
pub async fn serve(listener: tokio::net::TcpListener, dashboard: Arc<Dashboard>) -> std::io::Result<()> {
    axum::serve(listener, router(dashboard)).await
}

#[derive(Serialize)]
struct ErrorBody<'a> {
    error: &'a str,
}

#[derive(Serialize)]
struct AppsAnswer {
    /// Whether apps can be checked, added and removed from the settings page.
    manage_apps: bool,
    apps: Vec<AppStatus>,
}

/// An app to check: where it is, and its token if it wants one.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CheckRequest {
    base_url: String,
    token: Option<String>,
}

#[derive(Serialize)]
struct AddedBody<'a> {
    name: &'a str,
    summary: Option<String>,
}

#[derive(Serialize)]
struct RefusedBody {
    error: &'static str,
    report: CheckReport,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DataRequest {
    sources: Vec<SourceRequest>,
}

#[derive(Serialize)]
struct DataAnswer {
    /// The server's clock, so the browser can tell how old things are without trusting its own.
    now: String,
    /// The saved layout's `rev`, so an open page notices a layout saved elsewhere.
    layout_rev: u64,
    sources: Vec<SourceAnswer>,
}

#[derive(Serialize)]
struct ConflictBody {
    error: &'static str,
    rev: u64,
}

#[derive(Serialize)]
struct InvalidBody {
    error: &'static str,
    problems: Vec<String>,
}

/// A JSON answer that browsers must not keep or guess the type of.
fn api<T: Serialize>(status: StatusCode, body: &T) -> Response {
    let mut response = (status, Json(body)).into_response();
    let headers = response.headers_mut();
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    response
}

fn error(status: StatusCode, message: &str) -> Response {
    api(status, &ErrorBody { error: message })
}

async fn get_apps(State(dashboard): State<Arc<Dashboard>>, Query(query): Query<HashMap<String, String>>) -> Response {
    let refresh = query.get("refresh").is_some_and(|value| value == "1" || value == "true");
    let apps = dashboard.hub.apps(refresh).await;
    api(StatusCode::OK, &AppsAnswer { manage_apps: dashboard.manage_apps, apps })
}

/// Calls an app the way the dashboard would and reports what is wrong. Nothing is added.
async fn post_check(
    State(dashboard): State<Arc<Dashboard>>,
    body: Result<Json<CheckRequest>, JsonRejection>,
) -> Response {
    if !dashboard.manage_apps {
        return error(StatusCode::FORBIDDEN, MANAGE_OFF);
    }
    let request = match body {
        Ok(Json(request)) => request,
        Err(rejection) => return error(rejection.status(), &rejection.body_text()),
    };
    let entry =
        AppEntry { name: "check".to_string(), base_url: request.base_url, public_url: None, token: request.token };
    match AppConfig::new(entry, true) {
        Ok(app) => api(StatusCode::OK, &dashboard.hub.check(&app).await),
        Err(problems) => {
            api(StatusCode::BAD_REQUEST, &InvalidBody { error: "the app's address is not valid", problems })
        }
    }
}

/// Adds an app, if it passes the check. It is kept in the apps file and shows up at once.
async fn post_app(State(dashboard): State<Arc<Dashboard>>, body: Result<Json<AppEntry>, JsonRejection>) -> Response {
    if !dashboard.manage_apps {
        return error(StatusCode::FORBIDDEN, MANAGE_OFF);
    }
    let entry = match body {
        Ok(Json(entry)) => entry,
        Err(rejection) => return error(rejection.status(), &rejection.body_text()),
    };
    let app = match AppConfig::new(entry, true) {
        Ok(app) => app,
        Err(problems) => {
            return api(StatusCode::BAD_REQUEST, &InvalidBody { error: "the app is not valid", problems });
        }
    };
    let taken = || error(StatusCode::CONFLICT, &format!("there is already an app called {:?}", app.name));
    if dashboard.hub.configs().iter().any(|known| known.name == app.name) {
        return taken();
    }

    let report = dashboard.hub.check(&app).await;
    if !report.ok {
        let refused = RefusedBody { error: "the app does not pass the check yet, so it was not added", report };
        return api(StatusCode::UNPROCESSABLE_ENTITY, &refused);
    }
    if dashboard.hub.add(app.clone()).is_err() {
        return taken();
    }
    if let Err(e) = dashboard.save_apps() {
        // Not kept: do not pretend. The reason names a path on the server, so it goes to the log.
        eprintln!("tileboard: cannot save the apps file: {e}");
        let _ = dashboard.hub.remove(&app.name);
        return error(StatusCode::INTERNAL_SERVER_ERROR, "the app could not be saved; the server log says why");
    }
    api(StatusCode::CREATED, &AddedBody { name: &app.name, summary: report.summary })
}

/// Removes an app that was added from the settings page. Tiles that show it say so until they are changed.
async fn delete_app(State(dashboard): State<Arc<Dashboard>>, Path(name): Path<String>) -> Response {
    if !dashboard.manage_apps {
        return error(StatusCode::FORBIDDEN, MANAGE_OFF);
    }
    match dashboard.hub.remove(&name) {
        Err(AppChangeError::InConfigFile) => error(
            StatusCode::FORBIDDEN,
            &format!("{name:?} is written in config.toml; take it out there and restart the dashboard"),
        ),
        Err(_) => error(StatusCode::NOT_FOUND, &format!("there is no app called {name:?}")),
        Ok(()) => match dashboard.save_apps() {
            Ok(()) => api(StatusCode::OK, &AddedBody { name: &name, summary: None }),
            Err(e) => {
                eprintln!("tileboard: cannot save the apps file: {e}");
                error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "the app was removed for now but the change could not be saved",
                )
            }
        },
    }
}

/// The connect kit: one text with everything a developer or an agent needs, this dashboard's own address filled in.
async fn connect_kit(State(dashboard): State<Arc<Dashboard>>, headers: HeaderMap) -> Response {
    // The address the reader used to get here is the one they can use for the next calls.
    let host = headers.get(HOST).and_then(|host| host.to_str().ok()).filter(|host| {
        !host.is_empty()
            && host.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b':' | b'[' | b']'))
    });
    let address = format!("http://{}", host.unwrap_or("ADDRESS-OF-THE-DASHBOARD"));
    let connect = if dashboard.manage_apps { CONNECT_BY_CALL } else { CONNECT_BY_OWNER };
    let text = format!("{}{SPEC}", CONNECT_KIT.replace("{{CONNECT}}", connect).replace("{{DASHBOARD}}", &address));
    text_answer(text, "text/markdown; charset=utf-8")
}

const CONNECT_BY_CALL: &str = "Register the app on the dashboard. The dashboard checks it first and refuses an app that does not pass:

   ```sh
   curl -s -X POST {{DASHBOARD}}/api/apps -H \"Content-Type: application/json\" -d '{\"name\": \"my-app\", \"base_url\": \"http://ADDRESS-OF-THE-APP\"}'
   ```

   `name` is the owner's short name for the app (lowercase letters, digits, `_`, `-`). Tiles refer to it, so it must not change later. Optional: `\"public_url\"` (where a browser reaches the app, if that differs, for the links on tiles) and `\"token\"`. The owner can do the same by hand on the dashboard: Settings, then Connect an app.";

const CONNECT_BY_OWNER: &str = "This dashboard does not let apps be added from outside. Give its owner this block for the dashboard's `config.toml` (then they restart the dashboard):

   ```toml
   [[apps]]
   name = \"my-app\"                             # the owner's short name; tiles refer to it
   base_url = \"http://ADDRESS-OF-THE-APP\"
   # public_url = \"...\"                        # where a browser reaches the app, if that differs
   # token = \"...\"                             # only if the app asks for one
   ```";

fn text_answer(text: String, content_type: &'static str) -> Response {
    let mut response = text.into_response();
    let headers = response.headers_mut();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static(content_type));
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    response
}

fn schema(text: &'static str) -> Response {
    text_answer(text.to_string(), "application/schema+json")
}

async fn post_data(
    State(dashboard): State<Arc<Dashboard>>,
    body: Result<Json<DataRequest>, JsonRejection>,
) -> Response {
    let request = match body {
        Ok(Json(request)) => request,
        Err(rejection) => return error(rejection.status(), &rejection.body_text()),
    };
    if request.sources.len() > MAX_SOURCES {
        return error(StatusCode::BAD_REQUEST, &format!("one request can name at most {MAX_SOURCES} sources"));
    }
    let sources = dashboard.hub.data(request.sources).await;
    let answer = DataAnswer { now: rfc3339::format(rfc3339::now()), layout_rev: dashboard.layout.rev(), sources };
    api(StatusCode::OK, &answer)
}

async fn get_layout(State(dashboard): State<Arc<Dashboard>>) -> Response {
    api(StatusCode::OK, &dashboard.layout.get())
}

async fn put_layout(State(dashboard): State<Arc<Dashboard>>, body: Result<Json<Layout>, JsonRejection>) -> Response {
    let layout = match body {
        Ok(Json(layout)) => layout,
        Err(rejection) => return error(rejection.status(), &rejection.body_text()),
    };
    match dashboard.layout.save(layout) {
        Ok(saved) => api(StatusCode::OK, &saved),
        Err(SaveError::Conflict { current }) => api(
            StatusCode::CONFLICT,
            &ConflictBody { error: "the layout was changed somewhere else; load it again", rev: current },
        ),
        Err(SaveError::Invalid(problems)) => {
            api(StatusCode::BAD_REQUEST, &InvalidBody { error: "the layout is not valid", problems })
        }
        Err(SaveError::Io(why)) => {
            // The reason names a path on the server: it goes to the log, not to the browser.
            eprintln!("tileboard: {why}");
            error(StatusCode::INTERNAL_SERVER_ERROR, "the layout could not be saved; the server log says why")
        }
    }
}

fn web_file(web: &Web, file: &WebFile) -> Response {
    let text = match web.read(file) {
        Ok(text) => text,
        Err(e) => {
            eprintln!("tileboard: cannot read the frontend file {}: {e}", file.name);
            return error(StatusCode::INTERNAL_SERVER_ERROR, "a frontend file is missing; the server log says which");
        }
    };
    let mut response = text.into_owned().into_response();
    let headers = response.headers_mut();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static(file.content_type));
    // Always ask the server: the files are tiny and a stale frontend is confusing.
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    headers.insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    headers.insert(REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    headers.insert(CONTENT_SECURITY_POLICY, HeaderValue::from_static(PAGE_POLICY));
    response
}
