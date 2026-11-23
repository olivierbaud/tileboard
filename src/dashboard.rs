//! The dashboard server: the browser's API and the frontend files.
//!
//! | Address | What |
//! |---|---|
//! | `GET /api/apps` | every configured app with its catalogue (`?refresh=1` reads them again) |
//! | `POST /api/data` | the data of the sources the browser names, cached per refresh |
//! | `GET /api/layout` | the saved layout |
//! | `PUT /api/layout` | save the whole layout (must carry the `rev` it started from) |
//! | `GET /`, `/app.js`, `/style.css` | the frontend |

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::rejection::JsonRejection;
use axum::extract::{Query, State};
use axum::http::header::{
    CACHE_CONTROL, CONTENT_SECURITY_POLICY, CONTENT_TYPE, REFERRER_POLICY, X_CONTENT_TYPE_OPTIONS,
};
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::hub::{AppStatus, Hub, SourceAnswer, SourceRequest};
use crate::layout::{Layout, LayoutStore, SaveError};
use crate::rfc3339;
use crate::web::{FILES, Web, WebFile};

/// The most sources one data request may name.
pub const MAX_SOURCES: usize = 200;

/// The page may only load its own files: no inline script, nothing from another site, no framing.
const PAGE_POLICY: &str = "default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' data:; \
                           connect-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'";

pub struct Dashboard {
    pub hub: Hub,
    pub layout: LayoutStore,
    pub web: Web,
}

pub fn router(dashboard: Arc<Dashboard>) -> Router {
    let mut router = Router::new()
        .route("/api/apps", get(get_apps))
        .route("/api/data", post(post_data))
        .route("/api/layout", get(get_layout).put(put_layout));
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
    apps: Vec<AppStatus>,
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
    api(StatusCode::OK, &AppsAnswer { apps: dashboard.hub.apps(refresh).await })
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
