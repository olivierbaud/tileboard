//! The hub talks to the apps. It keeps every catalogue and every data answer for
//! as long as it is fresh, so any number of tiles and browsers cost one call per
//! source per refresh. The browser never talks to an app: tokens stay here.
//!
//! An app that is down must not slow the dashboard: once a call has failed, the
//! hub answers at once with what it has kept and tries again in the background.

use std::collections::{BTreeMap, HashMap};
use std::future::Future;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex as AsyncMutex;

use crate::client::HttpClient;
use crate::config::{AppConfig, Token};
use crate::contract::{CONTRACT, Catalogue, Entry, Source, Status, Value, is_link};
use crate::rfc3339;

/// A forced catalogue refresh is ignored if the last try was this recent.
const REFRESH_FLOOR: Duration = Duration::from_secs(1);
/// The most (source, params) answers kept per app.
const MAX_ANSWERS_PER_APP: usize = 256;
/// The most problems of a bad catalogue that are spelled out.
const MAX_PROBLEMS_SHOWN: usize = 3;

/// How patient the hub is. `Default` is what the dashboard runs with; tests shorten it.
#[derive(Debug, Clone, Copy)]
pub struct Timings {
    /// An app gets this long to answer.
    pub timeout: Duration,
    /// After a failed call, the next try waits this long (or one refresh, if that is shorter).
    pub retry: Duration,
    /// A good catalogue is read again after this long.
    pub catalogue_good_for: Duration,
}

impl Default for Timings {
    fn default() -> Timings {
        Timings {
            timeout: Duration::from_secs(5),
            retry: Duration::from_secs(15),
            catalogue_good_for: Duration::from_secs(300),
        }
    }
}

pub struct Hub {
    client: HttpClient,
    apps: Vec<Arc<App>>,
}

struct App {
    config: AppConfig,
    timings: Timings,
    catalogue: Arc<Kept<Arc<Catalogue>>>,
    /// One kept answer per (source, params).
    answers: Mutex<HashMap<String, Arc<Kept<Good>>>>,
}

/// A good data answer, already checked against the catalogue.
#[derive(Clone)]
struct Good {
    fetched_at: i64,
    updated_at: String,
    link: Option<String>,
    values: BTreeMap<String, OutEntry>,
}

// ------------------------------------------------------------------- kept

/// Something read from an app and kept: the last good value, and why the last try failed, if it did.
struct Kept<T> {
    state: Mutex<State<T>>,
    /// Held while a call to the app is under way, so there is only one at a time.
    calling: Arc<AsyncMutex<()>>,
}

struct State<T> {
    tried: Option<Instant>,
    tried_at: Option<i64>,
    /// The last good value. It stays when a later call fails.
    good: Option<T>,
    /// Why the last try failed.
    error: Option<String>,
}

/// What a [`Kept`] holds at one moment.
struct Snapshot<T> {
    tried_at: Option<i64>,
    good: Option<T>,
    error: Option<String>,
}

impl<T: Clone + Send + 'static> Kept<T> {
    fn new() -> Arc<Kept<T>> {
        let state = State { tried: None, tried_at: None, good: None, error: None };
        Arc::new(Kept { state: Mutex::new(state), calling: Arc::default() })
    }

    fn state(&self) -> MutexGuard<'_, State<T>> {
        // Nothing here can leave the state half-changed, so a poisoned lock is still good.
        self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn snapshot(&self) -> Snapshot<T> {
        let state = self.state();
        Snapshot { tried_at: state.tried_at, good: state.good.clone(), error: state.error.clone() }
    }

    fn store(&self, result: Result<T, String>) {
        let mut state = self.state();
        match result {
            Ok(good) => {
                state.good = Some(good);
                state.error = None;
            }
            Err(error) => state.error = Some(error),
        }
        state.tried = Some(Instant::now());
        state.tried_at = Some(rfc3339::now());
    }

    /// Whether it is time to call the app again, and whether the last call failed.
    fn due(&self, good_for: Duration, retry: Duration, now: bool) -> (bool, bool) {
        let state = self.state();
        let failing = state.tried.is_some() && state.error.is_some();
        let wait = if now {
            REFRESH_FLOOR
        } else if failing {
            retry
        } else {
            good_for
        };
        (state.tried.is_none_or(|tried| tried.elapsed() >= wait), failing)
    }

    /// What is kept, after calling the app again (`fetch`) if it is time.
    ///
    /// - Never called, or the last call worked: the caller waits for the call. A value is good
    ///   for `good_for`.
    /// - The last call failed: the caller gets what is kept at once, and the next try (after
    ///   `retry`) runs in the background, so a dead app never makes anyone wait twice.
    /// - `now`: call again even if it is not time (but not twice within a second), and wait for it.
    async fn get<F, Fut>(self: &Arc<Self>, good_for: Duration, retry: Duration, now: bool, fetch: F) -> Snapshot<T>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Result<T, String>> + Send + 'static,
    {
        let (due, failing) = self.due(good_for, retry, now);
        if !due {
            return self.snapshot();
        }
        if failing && !now {
            if let Ok(calling) = self.calling.clone().try_lock_owned() {
                let kept = self.clone();
                tokio::spawn(async move {
                    let result = fetch().await;
                    kept.store(result);
                    drop(calling);
                });
            }
            return self.snapshot();
        }

        let _calling = self.calling.lock().await;
        // Another request may have made the call while this one waited for its turn.
        if self.due(good_for, retry, now).0 {
            let result = fetch().await;
            self.store(result);
        }
        self.snapshot()
    }
}

// ------------------------------------------------------- what the browser gets

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
    #[serde(skip_serializing_if = "Option::is_none", serialize_with = "crate::contract::plain::series")]
    pub series: Option<Vec<Option<f64>>>,
    /// Set when the app sent something for this field that does not fit it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl OutEntry {
    fn empty(error: Option<String>) -> OutEntry {
        OutEntry { value: Value::Null, status: None, series: None, error }
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

// -------------------------------------------------------------------- hub

impl Hub {
    pub fn new(apps: Vec<AppConfig>) -> Hub {
        Hub::with_timings(apps, Timings::default())
    }

    pub fn with_timings(apps: Vec<AppConfig>, timings: Timings) -> Hub {
        let apps = apps
            .into_iter()
            .map(|config| Arc::new(App { config, timings, catalogue: Kept::new(), answers: Mutex::default() }))
            .collect();
        Hub { client: HttpClient::new(timings.timeout), apps }
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
    /// The app's catalogue as kept, read again first if it is time (or if `now` asks for it).
    async fn catalogue(&self, client: &HttpClient, now: bool) -> Snapshot<Arc<Catalogue>> {
        let (client, token) = (client.clone(), self.config.token.clone());
        let url = format!("{}/widgets", self.config.base_url);
        let fetch = move || async move { fetch_catalogue(&client, &url, token.as_ref()).await.map(Arc::new) };
        self.catalogue.get(self.timings.catalogue_good_for, self.timings.retry, now, fetch).await
    }

    async fn status(&self, client: &HttpClient, refresh: bool) -> AppStatus {
        let kept = self.catalogue(client, refresh).await;
        AppStatus {
            name: self.config.name.clone(),
            ok: kept.error.is_none(),
            error: kept.error,
            checked_at: kept.tried_at.map(rfc3339::format),
            catalogue: kept.good.map(|catalogue| (*catalogue).clone()),
        }
    }

    async fn data(&self, client: &HttpClient, request: &SourceRequest) -> SourceAnswer {
        // An old catalogue is still used when the app is unreachable: the data
        // call then fails too, and the answer carries the last good values.
        let catalogue = self.catalogue(client, false).await;
        let Some(catalogue_good) = catalogue.good else {
            let error = catalogue.error.unwrap_or_else(|| "the app has no catalogue".to_string());
            return SourceAnswer::failed(request, error);
        };
        let Some(source) = catalogue_good.source(&request.source) else {
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
        let query = if query.is_empty() { String::new() } else { format!("?{}", query.join("&")) };
        let path = format!("{}{query}", source.id);

        let fetch = {
            let (client, token, source) = (client.clone(), self.config.token.clone(), source.clone());
            let url = format!("{}/widgets/{path}", self.config.base_url);
            let public_url = self.config.public_url.clone();
            move || async move { fetch_data(&client, &url, token.as_ref(), &source, &public_url).await }
        };
        let refresh = Duration::from_secs(source.refresh_s.max(1));
        let kept = self.answer(&path).get(refresh, refresh.min(self.timings.retry), false, fetch).await;

        SourceAnswer {
            app: request.app.clone(),
            source: request.source.clone(),
            params,
            ok: kept.error.is_none(),
            error: kept.error,
            refresh_s: Some(source.refresh_s),
            fetched_at: kept.good.as_ref().map(|good| rfc3339::format(good.fetched_at)),
            updated_at: kept.good.as_ref().map(|good| good.updated_at.clone()),
            link: kept.good.as_ref().and_then(|good| good.link.clone()),
            values: kept.good.map(|good| good.values).unwrap_or_default(),
        }
    }

    /// The kept answer of one (source, params), made on first use.
    fn answer(&self, key: &str) -> Arc<Kept<Good>> {
        let mut answers = self.answers.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if !answers.contains_key(key) && answers.len() >= MAX_ANSWERS_PER_APP {
            // Too many kept: forget every answer nobody is using right now.
            answers.retain(|_, kept| Arc::strong_count(kept) > 1);
        }
        answers.entry(key.to_string()).or_insert_with(Kept::new).clone()
    }
}

// ------------------------------------------------------------ calling apps

async fn fetch_catalogue(client: &HttpClient, url: &str, token: Option<&Token>) -> Result<Catalogue, String> {
    let body = client.get(url, token.map(Token::reveal)).await.map_err(|e| e.to_string())?;
    let doc: serde_json::Value =
        serde_json::from_slice(&body).map_err(|_| "the app's /widgets answer is not JSON".to_string())?;
    match doc.get("contract").and_then(serde_json::Value::as_u64) {
        Some(version) if version == u64::from(CONTRACT) => {}
        Some(version) => {
            return Err(format!("the app speaks widget contract {version}; this dashboard speaks contract {CONTRACT}"));
        }
        None => {
            return Err(format!("the app's /widgets answer is not a contract {CONTRACT} catalogue"));
        }
    }
    let catalogue: Catalogue =
        serde_json::from_value(doc).map_err(|e| format!("the app's catalogue does not follow the spec: {e}"))?;

    let problems = catalogue.problems();
    if problems.is_empty() {
        return Ok(catalogue);
    }
    let mut shown = problems.iter().take(MAX_PROBLEMS_SHOWN).cloned().collect::<Vec<_>>().join("; ");
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
    token: Option<&Token>,
    source: &Source,
    public_url: &str,
) -> Result<Good, String> {
    let body = client.get(url, token.map(Token::reveal)).await.map_err(|e| e.to_string())?;
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
        .map(|link| if link.starts_with('/') { format!("{public_url}{link}") } else { link.to_string() });

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

    Ok(Good { fetched_at: rfc3339::now(), updated_at: raw.updated_at, link, values })
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
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    const LONG: Duration = Duration::from_secs(3600);
    const NO_WAIT: Duration = Duration::ZERO;

    /// A `fetch` that counts its calls and gives `result`.
    fn counted(
        calls: &Arc<AtomicUsize>,
        result: Result<u32, &'static str>,
    ) -> impl FnOnce() -> std::future::Ready<Result<u32, String>> + Send + 'static {
        let calls = calls.clone();
        move || {
            calls.fetch_add(1, Ordering::SeqCst);
            std::future::ready(result.map_err(str::to_string))
        }
    }

    #[test]
    fn query_values_are_percent_encoded() {
        assert_eq!(percent_encode("atlas"), "atlas");
        assert_eq!(percent_encode("a-b_c.d~e"), "a-b_c.d~e");
        assert_eq!(percent_encode("two words"), "two%20words");
        assert_eq!(percent_encode("a&b=c#d?e/f+g%"), "a%26b%3Dc%23d%3Fe%2Ff%2Bg%25");
        assert_eq!(percent_encode("é"), "%C3%A9");
        assert_eq!(percent_encode(""), "");
    }

    #[tokio::test]
    async fn a_good_value_is_kept_until_it_is_old() {
        let (kept, calls) = (Kept::<u32>::new(), Arc::new(AtomicUsize::new(0)));
        let first = kept.get(LONG, LONG, false, counted(&calls, Ok(1))).await;
        assert_eq!((first.good, first.error), (Some(1), None));
        // Still good: the app is not called again.
        let second = kept.get(LONG, LONG, false, counted(&calls, Ok(2))).await;
        assert_eq!(second.good, Some(1));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        // Old: called again, and the caller waits for the new value.
        let third = kept.get(NO_WAIT, LONG, false, counted(&calls, Ok(3))).await;
        assert_eq!(third.good, Some(3));
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn a_failed_call_keeps_the_last_good_value() {
        let (kept, calls) = (Kept::<u32>::new(), Arc::new(AtomicUsize::new(0)));
        kept.get(LONG, LONG, false, counted(&calls, Ok(1))).await;
        let failed = kept.get(NO_WAIT, LONG, false, counted(&calls, Err("down"))).await;
        assert_eq!((failed.good, failed.error.as_deref()), (Some(1), Some("down")));
        assert!(failed.tried_at.is_some());
    }

    #[tokio::test]
    async fn a_first_call_that_fails_is_waited_for() {
        let (kept, calls) = (Kept::<u32>::new(), Arc::new(AtomicUsize::new(0)));
        let failed = kept.get(LONG, LONG, false, counted(&calls, Err("down"))).await;
        assert_eq!((failed.good, failed.error.as_deref()), (None, Some("down")));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        // Not time to retry yet: no call.
        kept.get(LONG, LONG, false, counted(&calls, Ok(1))).await;
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn after_a_failure_the_retry_runs_in_the_background() {
        let (kept, calls) = (Kept::<u32>::new(), Arc::new(AtomicUsize::new(0)));
        kept.get(LONG, NO_WAIT, false, counted(&calls, Err("down"))).await;

        // Time to retry, and the app is back: the caller still gets the old state at once...
        let at_once = kept.get(LONG, NO_WAIT, false, counted(&calls, Ok(7))).await;
        assert_eq!((at_once.good, at_once.error.as_deref()), (None, Some("down")));
        // ...and the next caller sees what the background call brought.
        tokio::task::yield_now().await;
        tokio::time::sleep(Duration::from_millis(20)).await;
        let after = kept.get(LONG, LONG, false, counted(&calls, Ok(8))).await;
        assert_eq!((after.good, after.error), (Some(7), None));
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn asking_for_it_now_calls_again_and_waits() {
        let (kept, calls) = (Kept::<u32>::new(), Arc::new(AtomicUsize::new(0)));
        kept.get(LONG, LONG, false, counted(&calls, Err("down"))).await;
        // Within a second of the last try, even "now" does not call again.
        let too_soon = kept.get(LONG, LONG, true, counted(&calls, Ok(1))).await;
        assert_eq!(too_soon.good, None);
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        tokio::time::sleep(REFRESH_FLOOR + Duration::from_millis(50)).await;
        let now = kept.get(LONG, LONG, true, counted(&calls, Ok(2))).await;
        assert_eq!((now.good, now.error), (Some(2), None));
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn many_callers_at_once_make_one_call() {
        let (kept, calls) = (Kept::<u32>::new(), Arc::new(AtomicUsize::new(0)));
        let slow = |calls: &Arc<AtomicUsize>| {
            let calls = calls.clone();
            move || async move {
                calls.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(50)).await;
                Ok(5)
            }
        };
        let callers: Vec<_> = (0..10)
            .map(|_| {
                let (kept, fetch) = (kept.clone(), slow(&calls));
                tokio::spawn(async move { kept.get(LONG, LONG, false, fetch).await.good })
            })
            .collect();
        for caller in callers {
            assert_eq!(caller.await.unwrap(), Some(5));
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
