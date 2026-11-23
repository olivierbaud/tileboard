//! The hub talks to the apps. It keeps every catalogue and every data answer for
//! as long as it is fresh, so any number of tiles and browsers cost one call per
//! source per refresh. The browser never talks to an app: tokens stay here.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex as AsyncMutex;

use crate::client::HttpClient;
use crate::config::AppConfig;
use crate::contract::{CONTRACT, Catalogue, Entry, Source, Status, Value, is_link};
use crate::rfc3339;

/// An app gets this long to answer.
pub const TIMEOUT: Duration = Duration::from_secs(5);
/// A good catalogue is read again after this long.
const CATALOGUE_TTL: Duration = Duration::from_secs(300);
/// After a failed call, the next try waits this long (or one refresh, if that is shorter).
const RETRY: Duration = Duration::from_secs(15);
/// A forced catalogue refresh is ignored if the last try was this recent.
const REFRESH_FLOOR: Duration = Duration::from_secs(1);
/// The most (source, params) answers kept per app.
const MAX_ANSWERS_PER_APP: usize = 256;
/// The most problems of a bad catalogue that are spelled out.
const MAX_PROBLEMS_SHOWN: usize = 3;

pub struct Hub {
    client: HttpClient,
    apps: Vec<Arc<App>>,
}

struct App {
    config: AppConfig,
    catalogue: AsyncMutex<CatalogueSlot>,
    /// One slot per (source, params). The async lock of a slot is held during
    /// its call, so two requests for the same thing make one call.
    answers: Mutex<HashMap<String, Arc<AsyncMutex<AnswerSlot>>>>,
}

#[derive(Default)]
struct CatalogueSlot {
    tried: Option<Instant>,
    checked_at: Option<i64>,
    /// The last good catalogue. Kept when the app goes away, so its tiles can still be drawn.
    good: Option<Arc<Catalogue>>,
    /// Why the last try failed.
    error: Option<String>,
}

#[derive(Default)]
struct AnswerSlot {
    tried: Option<Instant>,
    /// The last good answer. Kept when a later call fails.
    good: Option<Good>,
    /// Why the last try failed.
    error: Option<String>,
}

#[derive(Clone)]
struct Good {
    fetched_at: i64,
    updated_at: String,
    link: Option<String>,
    values: BTreeMap<String, OutEntry>,
}

/// One app as the browser sees it (`GET /api/apps`).
#[derive(Debug, Clone, Serialize)]
pub struct AppStatus {
    pub name: String,
    /// Whether the last try to read the catalogue worked.
    pub ok: bool,
    pub error: Option<String>,
    pub checked_at: Option<String>,
    /// The last good catalogue, even if the app is unreachable now.
    pub catalogue: Option<Catalogue>,
}

/// One source the browser wants data for.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceRequest {
    pub app: String,
    pub source: String,
    #[serde(default)]
    pub params: BTreeMap<String, String>,
}

/// The data of one source as the browser sees it.
#[derive(Debug, Clone, Serialize)]
pub struct SourceAnswer {
    pub app: String,
    pub source: String,
    /// Every param of the source with the value used (defaults filled in).
    pub params: BTreeMap<String, String>,
    /// Whether the last call to the app worked. If not, `values` are the last good ones, if any.
    pub ok: bool,
    pub error: Option<String>,
    pub refresh_s: Option<u64>,
    /// When the dashboard last got a good answer.
    pub fetched_at: Option<String>,
    /// When the app says it last refreshed the data.
    pub updated_at: Option<String>,
    /// Where a click goes, as a full address.
    pub link: Option<String>,
    pub values: BTreeMap<String, OutEntry>,
}

/// One field's value as the browser sees it: checked against the catalogue, with its status worked out.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OutEntry {
    pub value: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<Status>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "crate::contract::plain::series"
    )]
    pub series: Option<Vec<Option<f64>>>,
    /// Set when the app sent something for this field that does not fit it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl OutEntry {
    fn empty(error: Option<String>) -> OutEntry {
        OutEntry {
            value: Value::Null,
            status: None,
            series: None,
            error,
        }
    }
}

impl SourceAnswer {
    fn failed(request: &SourceRequest, error: String) -> SourceAnswer {
        SourceAnswer {
            app: request.app.clone(),
            source: request.source.clone(),
            params: request.params.clone(),
            ok: false,
            error: Some(error),
            refresh_s: None,
            fetched_at: None,
            updated_at: None,
            link: None,
            values: BTreeMap::new(),
        }
    }
}

impl Hub {
    pub fn new(apps: Vec<AppConfig>) -> Hub {
        Hub::with_timeout(apps, TIMEOUT)
    }

    pub fn with_timeout(apps: Vec<AppConfig>, timeout: Duration) -> Hub {
        let apps = apps
            .into_iter()
            .map(|config| {
                Arc::new(App {
                    config,
                    catalogue: AsyncMutex::default(),
                    answers: Mutex::default(),
                })
            })
            .collect();
        Hub {
            client: HttpClient::new(timeout),
            apps,
        }
    }

    /// Every configured app with its catalogue. `refresh` reads the catalogues again now.
    pub async fn apps(&self, refresh: bool) -> Vec<AppStatus> {
        let calls: Vec<_> = self
            .apps
            .iter()
            .map(|app| {
                let (app, client) = (app.clone(), self.client.clone());
                tokio::spawn(async move { app.status(&client, refresh).await })
            })
            .collect();

        let mut statuses = Vec::new();
        for (app, call) in self.apps.iter().zip(calls) {
            statuses.push(call.await.unwrap_or_else(|_| AppStatus {
                name: app.config.name.clone(),
                ok: false,
                error: Some("the dashboard failed while reading this app".to_string()),
                checked_at: None,
                catalogue: None,
            }));
        }
        statuses
    }

    /// The data of each requested source, in the order asked. All calls run side by side.
    pub async fn data(&self, requests: Vec<SourceRequest>) -> Vec<SourceAnswer> {
        let calls: Vec<_> = requests
            .iter()
            .map(|request| {
                let app = self.apps.iter().find(|app| app.config.name == request.app).cloned();
                let (client, request) = (self.client.clone(), request.clone());
                tokio::spawn(async move {
                    match app {
                        Some(app) => app.data(&client, &request).await,
                        None => {
                            SourceAnswer::failed(&request, format!("there is no app {:?} in the config", request.app))
                        }
                    }
                })
            })
            .collect();

        let mut answers = Vec::new();
        for (request, call) in requests.iter().zip(calls) {
            answers.push(call.await.unwrap_or_else(|_| {
                SourceAnswer::failed(request, "the dashboard failed while reading this source".to_string())
            }));
        }
        answers
    }
}

impl App {
    fn token(&self) -> Option<&str> {
        self.config.token.as_ref().map(|token| token.reveal())
    }

    /// Reads the catalogue if the kept one is too old (or `refresh` asks for it), and returns the slot's state.
    async fn catalogue(
        &self,
        client: &HttpClient,
        refresh: bool,
    ) -> (Option<Arc<Catalogue>>, Option<String>, Option<i64>) {
        let mut slot = self.catalogue.lock().await;
        let wait = match (refresh, &slot.error) {
            (true, _) => REFRESH_FLOOR,
            (false, None) => CATALOGUE_TTL,
            (false, Some(_)) => RETRY,
        };
        if slot.tried.is_none_or(|tried| tried.elapsed() >= wait) {
            match fetch_catalogue(client, &self.config.base_url, self.token()).await {
                Ok(catalogue) => {
                    slot.good = Some(Arc::new(catalogue));
                    slot.error = None;
                }
                Err(error) => slot.error = Some(error),
            }
            slot.tried = Some(Instant::now());
            slot.checked_at = Some(rfc3339::now());
        }
        (slot.good.clone(), slot.error.clone(), slot.checked_at)
    }

    async fn status(&self, client: &HttpClient, refresh: bool) -> AppStatus {
        let (catalogue, error, checked_at) = self.catalogue(client, refresh).await;
        AppStatus {
            name: self.config.name.clone(),
            ok: error.is_none(),
            error,
            checked_at: checked_at.map(rfc3339::format),
            catalogue: catalogue.map(|catalogue| (*catalogue).clone()),
        }
    }

    async fn data(&self, client: &HttpClient, request: &SourceRequest) -> SourceAnswer {
        // An old catalogue is still used when the app is unreachable: the call
        // below then fails too, and the answer carries the last good values.
        let (catalogue, error, _) = self.catalogue(client, false).await;
        let Some(catalogue) = catalogue else {
            let error = error.unwrap_or_else(|| "the app has no catalogue".to_string());
            return SourceAnswer::failed(request, error);
        };
        let Some(source) = catalogue.source(&request.source) else {
            return SourceAnswer::failed(request, format!("the app has no source {:?}", request.source));
        };

        // Every param of the source: the requested value, or its default. Others are dropped.
        let mut params = BTreeMap::new();
        let mut query = Vec::new();
        for param in &source.params {
            let value = request.params.get(&param.name).unwrap_or(&param.default);
            if !param.accepts(value) {
                return SourceAnswer::failed(request, format!("{value:?} is not a valid value for {:?}", param.name));
            }
            params.insert(param.name.clone(), value.clone());
            query.push(format!("{}={}", param.name, percent_encode(value)));
        }
        let query = if query.is_empty() {
            String::new()
        } else {
            format!("?{}", query.join("&"))
        };
        let path = format!("{}{query}", source.id);

        let slot = self.answer_slot(&path);
        let mut slot = slot.lock().await;
        let refresh = Duration::from_secs(source.refresh_s.max(1));
        let wait = if slot.error.is_none() {
            refresh
        } else {
            refresh.min(RETRY)
        };
        if slot.tried.is_none_or(|tried| tried.elapsed() >= wait) {
            let url = format!("{}/widgets/{path}", self.config.base_url);
            match fetch_data(client, &url, self.token(), source, &self.config.public_url).await {
                Ok(good) => {
                    slot.good = Some(good);
                    slot.error = None;
                }
                Err(error) => slot.error = Some(error),
            }
            slot.tried = Some(Instant::now());
        }

        let good = slot.good.clone();
        SourceAnswer {
            app: request.app.clone(),
            source: request.source.clone(),
            params,
            ok: slot.error.is_none(),
            error: slot.error.clone(),
            refresh_s: Some(source.refresh_s),
            fetched_at: good.as_ref().map(|good| rfc3339::format(good.fetched_at)),
            updated_at: good.as_ref().map(|good| good.updated_at.clone()),
            link: good.as_ref().and_then(|good| good.link.clone()),
            values: good.map(|good| good.values).unwrap_or_default(),
        }
    }

    /// The slot of one (source, params), made on first use.
    fn answer_slot(&self, key: &str) -> Arc<AsyncMutex<AnswerSlot>> {
        let mut answers = self.answers.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if !answers.contains_key(key) && answers.len() >= MAX_ANSWERS_PER_APP {
            // Too many kept: forget every answer nobody is waiting for right now.
            answers.retain(|_, slot| Arc::strong_count(slot) > 1);
        }
        answers.entry(key.to_string()).or_default().clone()
    }
}

async fn fetch_catalogue(client: &HttpClient, base_url: &str, token: Option<&str>) -> Result<Catalogue, String> {
    let body = client
        .get(&format!("{base_url}/widgets"), token)
        .await
        .map_err(|e| e.to_string())?;
    let doc: serde_json::Value =
        serde_json::from_slice(&body).map_err(|_| "the app's /widgets answer is not JSON".to_string())?;
    match doc.get("contract").and_then(serde_json::Value::as_u64) {
        Some(version) if version == u64::from(CONTRACT) => {}
        Some(version) => {
            return Err(format!(
                "the app speaks widget contract {version}; this dashboard speaks contract {CONTRACT}"
            ));
        }
        None => {
            return Err(format!(
                "the app's /widgets answer is not a contract {CONTRACT} catalogue"
            ));
        }
    }
    let catalogue: Catalogue =
        serde_json::from_value(doc).map_err(|e| format!("the app's catalogue does not follow the spec: {e}"))?;

    let problems = catalogue.problems();
    if problems.is_empty() {
        return Ok(catalogue);
    }
    let mut shown = problems
        .iter()
        .take(MAX_PROBLEMS_SHOWN)
        .cloned()
        .collect::<Vec<_>>()
        .join("; ");
    if problems.len() > MAX_PROBLEMS_SHOWN {
        shown.push_str(&format!("; and {} more", problems.len() - MAX_PROBLEMS_SHOWN));
    }
    Err(format!("the app's catalogue does not follow the spec: {shown}"))
}

/// A data answer before its entries are looked at one by one.
#[derive(Deserialize)]
struct RawData {
    updated_at: String,
    #[serde(default)]
    link: Option<serde_json::Value>,
    values: BTreeMap<String, serde_json::Value>,
}

async fn fetch_data(
    client: &HttpClient,
    url: &str,
    token: Option<&str>,
    source: &Source,
    public_url: &str,
) -> Result<Good, String> {
    let body = client.get(url, token).await.map_err(|e| e.to_string())?;
    let mut raw: RawData =
        serde_json::from_slice(&body).map_err(|e| format!("the app's answer does not follow the spec: {e}"))?;
    if rfc3339::parse(&raw.updated_at).is_none() {
        return Err("the app's answer does not follow the spec: updated_at is not a date-time".to_string());
    }

    // A link that is not an http(s) address or a path is dropped, not followed.
    let link = raw
        .link
        .as_ref()
        .and_then(|link| link.as_str())
        .filter(|link| is_link(link))
        .map(|link| {
            if link.starts_with('/') {
                format!("{public_url}{link}")
            } else {
                link.to_string()
            }
        });

    // One bad field does not take the others down: each entry is checked on its own.
    let mut values = BTreeMap::new();
    for field in &source.fields {
        let out = match raw.values.remove(&field.id) {
            None => OutEntry::empty(None),
            Some(sent) => match serde_json::from_value::<Entry>(sent) {
                Err(_) => OutEntry::empty(Some("the app sent something that is not an entry".to_string())),
                Ok(entry) => match field.check_entry(&entry) {
                    Err(problem) => OutEntry::empty(Some(problem)),
                    Ok(()) => OutEntry {
                        status: field.status_of(&entry),
                        value: entry.value,
                        series: entry.series,
                        error: None,
                    },
                },
            },
        };
        values.insert(field.id.clone(), out);
    }

    Ok(Good {
        fetched_at: rfc3339::now(),
        updated_at: raw.updated_at,
        link,
        values,
    })
}

/// Makes any text safe as a query-string value.
fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => out.push(byte as char),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_values_are_percent_encoded() {
        assert_eq!(percent_encode("atlas"), "atlas");
        assert_eq!(percent_encode("a-b_c.d~e"), "a-b_c.d~e");
        assert_eq!(percent_encode("two words"), "two%20words");
        assert_eq!(percent_encode("a&b=c#d?e/f+g%"), "a%26b%3Dc%23d%3Fe%2Ff%2Bg%25");
        assert_eq!(percent_encode("é"), "%C3%A9");
        assert_eq!(percent_encode(""), "");
    }
}
