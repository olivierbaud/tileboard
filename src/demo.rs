//! The demo app: a widget-contract app whose data is made up and moves on its own.
//!
//! Every value is a pure function of the clock: no state, nothing stored, and
//! tests can pick the moment. The demo runs fast so there is something to watch:
//! one made-up hour of weather and one made-up day of spending go by per minute.

use std::collections::HashMap;
use std::f64::consts::TAU;
use std::fmt;
use std::sync::LazyLock;

use axum::extract::{Path, Query};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;

use crate::contract::{
    CONTRACT, Catalogue, Data, Entry, Field, FieldType, Item, Param, ParamOption, ParamType, Preset, Source, Status,
    View,
};
use crate::layout::{FieldRef, Layout, Tile};
use crate::rfc3339;

// ---------------------------------------------------------------- catalogue

static CATALOGUE: LazyLock<Catalogue> = LazyLock::new(|| Catalogue {
    contract: CONTRACT,
    app: "demo".into(),
    title: "Demo app".into(),
    sources: vec![
        Source {
            id: "server".into(),
            title: "Server health".into(),
            refresh_s: 5,
            params: vec![
                choice("host", "Host", &[("atlas", "Atlas"), ("comet", "Comet")]),
                Param {
                    name: "points".into(),
                    label: Some("History points (5 to 120)".into()),
                    kind: ParamType::Number,
                    options: vec![],
                    default: "30".into(),
                },
            ],
            fields: vec![
                field("cpu", "CPU", FieldType::Number, Some("%"), Some((75.0, 90.0))),
                field("cpu_history", "CPU history", FieldType::Series, Some("%"), Some((75.0, 90.0))),
                field("memory", "Memory", FieldType::Number, Some("%"), Some((80.0, 92.0))),
                // Lower is worse: crit is below warn.
                field("disk_free", "Disk free", FieldType::Number, Some("GB"), Some((50.0, 20.0))),
                field("uptime", "Uptime", FieldType::Text, None, None),
                field("backup", "Last backup", FieldType::Status, None, None),
                field("services", "Services", FieldType::List, None, None),
            ],
        },
        Source {
            id: "weather".into(),
            title: "Weather".into(),
            refresh_s: 60,
            params: vec![choice(
                "place",
                "Place",
                &[("northaven", "Northaven"), ("saltmere", "Saltmere"), ("emberfall", "Emberfall")],
            )],
            fields: vec![
                field("temp", "Temperature", FieldType::Number, Some("°C"), None),
                field("feels_like", "Feels like", FieldType::Number, Some("°C"), None),
                field("condition", "Conditions", FieldType::Text, None, None),
                field("wind", "Wind", FieldType::Number, Some("km/h"), Some((40.0, 70.0))),
                field("rain_chance", "Chance of rain", FieldType::Number, Some("%"), None),
                field("temp_24h", "Temperature, last 24 h", FieldType::Series, Some("°C"), None),
                field("forecast", "Forecast", FieldType::List, Some("°C"), None),
            ],
        },
        Source {
            id: "spending".into(),
            title: "Spending".into(),
            refresh_s: 30,
            params: vec![choice("period", "Period", &[("week", "This week"), ("month", "This month")])],
            fields: vec![
                field("total", "Spent", FieldType::Number, Some("$"), None),
                // Lower is worse: crit is below warn.
                field("budget_left", "Budget left", FieldType::Number, Some("$"), Some((100.0, 0.0))),
                field("daily", "Spent per day", FieldType::Series, Some("$"), None),
                field("top_categories", "Top categories", FieldType::List, Some("$"), None),
                field("biggest", "Biggest category", FieldType::Text, None, None),
                field("state", "Budget state", FieldType::Status, None, None),
            ],
        },
    ],
    presets: vec![
        preset("CPU", "server", &[], &["cpu"], View::Number),
        preset("CPU history", "server", &[], &["cpu_history"], View::Sparkline),
        preset("Server overview", "server", &[], &["cpu", "memory", "disk_free", "uptime"], View::Table),
        preset("Last backup", "server", &[], &["backup"], View::Status),
        preset("Services", "server", &[], &["services"], View::List),
        preset("Comet CPU", "server", &[("host", "comet")], &["cpu"], View::Number),
        preset("Weather now", "weather", &[], &["temp", "condition"], View::Number),
        preset("Temperature, last 24 h", "weather", &[], &["temp_24h"], View::Sparkline),
        preset("Forecast", "weather", &[], &["forecast"], View::List),
        preset("Spent this week", "spending", &[], &["total", "budget_left"], View::Number),
        preset("Top categories", "spending", &[], &["top_categories"], View::List),
        preset(
            "Month so far",
            "spending",
            &[("period", "month")],
            &["total", "budget_left", "biggest", "state"],
            View::Table,
        ),
    ],
});

/// The demo app's catalogue (`GET /widgets`).
pub fn catalogue() -> &'static Catalogue {
    &CATALOGUE
}

/// The layout `--demo` starts with, so the first page is not empty.
/// `app` is the name the demo app has in the dashboard's config.
pub fn starter_layout(app: &str) -> Layout {
    // (label, view, x, y, w, h, source, params, fields)
    type Row = (
        &'static str,
        View,
        u32,
        u32,
        u32,
        u32,
        &'static str,
        &'static [(&'static str, &'static str)],
        &'static [&'static str],
    );
    const ROWS: &[Row] = &[
        ("CPU", View::Number, 0, 0, 3, 2, "server", &[], &["cpu"]),
        ("CPU history", View::Sparkline, 3, 0, 5, 2, "server", &[], &["cpu_history"]),
        ("Last backup", View::Status, 8, 0, 2, 2, "server", &[], &["backup"]),
        ("Comet CPU", View::Number, 10, 0, 2, 2, "server", &[("host", "comet")], &["cpu"]),
        ("Atlas", View::Table, 0, 2, 4, 3, "server", &[], &["cpu", "memory", "disk_free", "uptime"]),
        ("Services", View::List, 4, 2, 3, 3, "server", &[], &["services"]),
        ("Northaven", View::Number, 7, 2, 2, 3, "weather", &[], &["temp", "condition"]),
        ("Temperature, last 24 h", View::Sparkline, 9, 2, 3, 3, "weather", &[], &["temp_24h"]),
        ("This week", View::Number, 0, 5, 3, 3, "spending", &[], &["total", "budget_left"]),
        ("Spent per day", View::Sparkline, 3, 5, 5, 3, "spending", &[], &["daily"]),
        ("Top categories", View::List, 8, 5, 4, 3, "spending", &[], &["top_categories"]),
    ];

    let tiles = ROWS
        .iter()
        .enumerate()
        .map(|(i, &(label, view, x, y, w, h, source, params, fields))| Tile {
            id: format!("demo-{}", i + 1),
            label: label.to_string(),
            view,
            x,
            y,
            w,
            h,
            fields: fields
                .iter()
                .map(|field| FieldRef {
                    app: app.to_string(),
                    source: source.to_string(),
                    params: params.iter().map(|(name, value)| (name.to_string(), value.to_string())).collect(),
                    field: field.to_string(),
                })
                .collect(),
        })
        .collect();
    Layout { rev: 0, tiles }
}

/// A choice param. The first option is the default.
fn choice(name: &str, label: &str, options: &[(&str, &str)]) -> Param {
    Param {
        name: name.into(),
        label: Some(label.into()),
        kind: ParamType::Choice,
        options: options
            .iter()
            .map(|(value, label)| ParamOption { value: (*value).into(), label: Some((*label).into()) })
            .collect(),
        default: options[0].0.into(),
    }
}

/// A field. `thresholds` is (warn, crit).
fn field(id: &str, label: &str, kind: FieldType, unit: Option<&str>, thresholds: Option<(f64, f64)>) -> Field {
    Field {
        id: id.into(),
        label: label.into(),
        kind,
        unit: unit.map(Into::into),
        warn: thresholds.map(|(warn, _)| warn),
        crit: thresholds.map(|(_, crit)| crit),
    }
}

fn preset(title: &str, source: &str, params: &[(&str, &str)], fields: &[&str], view: View) -> Preset {
    Preset {
        title: title.into(),
        source: source.into(),
        fields: fields.iter().map(|id| (*id).into()).collect(),
        params: params.iter().map(|(name, value)| ((*name).into(), (*value).into())).collect(),
        view: Some(view),
    }
}

// --------------------------------------------------------------------- data

/// Why the demo app refuses a data request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DemoError {
    UnknownSource(String),
    BadParam { name: String, value: String },
}

impl DemoError {
    fn bad_param(name: &str, value: &str) -> DemoError {
        DemoError::BadParam { name: name.into(), value: value.into() }
    }

    pub fn status(&self) -> StatusCode {
        match self {
            DemoError::UnknownSource(_) => StatusCode::NOT_FOUND,
            DemoError::BadParam { .. } => StatusCode::BAD_REQUEST,
        }
    }
}

impl fmt::Display for DemoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DemoError::UnknownSource(id) => write!(f, "there is no source {id:?}"),
            DemoError::BadParam { name, value } => write!(f, "{value:?} is not a valid value for {name:?}"),
        }
    }
}

impl std::error::Error for DemoError {}

/// The params of one request: each param's value from the query, or its default.
type Params<'a> = HashMap<&'a str, &'a str>;

type Values = Vec<(&'static str, Entry)>;

/// The demo app's data for one source (`GET /widgets/<source>`) at Unix time `now`.
pub fn data(source_id: &str, query: &HashMap<String, String>, now: i64) -> Result<Data, DemoError> {
    let source = CATALOGUE.source(source_id).ok_or_else(|| DemoError::UnknownSource(source_id.to_string()))?;

    // Query params the source does not have are ignored.
    let mut params = Params::new();
    for param in &source.params {
        let value = query.get(&param.name).unwrap_or(&param.default);
        if !param.accepts(value) {
            return Err(DemoError::bad_param(&param.name, value));
        }
        params.insert(&param.name, value);
    }

    // The data changes once per refresh: every answer inside one refresh is the same.
    let t = now - now.rem_euclid(source.refresh_s as i64);
    let values = match source_id {
        "server" => server(t, &params)?,
        "weather" => weather(t, &params),
        "spending" => spending(t, &params),
        other => unreachable!("source {other:?} is in the catalogue but has no data"),
    };

    Ok(Data {
        updated_at: rfc3339::format(t),
        link: Some(format!("/#{source_id}")),
        values: values.into_iter().map(|(id, entry)| (id.to_string(), entry)).collect(),
    })
}

struct Host {
    seed: u64,
    cpu: f64,
    /// How wild the CPU is: 1.0 is busy, less is calmer.
    swing: f64,
    memory: f64,
    /// Free disk in GB: (just after a clean-up, just before the next).
    disk: (f64, f64),
    /// Seconds this host runs ahead, so the hosts do not move together.
    ahead: i64,
    /// (seconds up at Unix time 0, days between restarts).
    uptime: (i64, i64),
}

const ATLAS: Host =
    Host { seed: 1, cpu: 55.0, swing: 1.0, memory: 62.0, disk: (120.0, 12.0), ahead: 0, uptime: (3_100_000, 37) };

const COMET: Host =
    Host { seed: 2, cpu: 24.0, swing: 0.5, memory: 41.0, disk: (380.0, 300.0), ahead: 290, uptime: (800_000, 11) };

fn cpu(host: &Host, t: i64) -> f64 {
    let load = 26.0 * wave(t + host.ahead, 240) + 12.0 * wave(t, 67) + 10.0 * (noise(host.seed, t / 5) - 0.5);
    round1((host.cpu + host.swing * load).clamp(1.0, 100.0))
}

/// The `points` param: a whole number from 5 to 120.
fn history_points(text: &str) -> Option<i64> {
    let n: f64 = text.parse().ok()?;
    (n.fract() == 0.0 && (5.0..=120.0).contains(&n)).then_some(n as i64)
}

fn server(t: i64, params: &Params) -> Result<Values, DemoError> {
    let host = if params["host"] == "comet" { &COMET } else { &ATLAS };
    let points = history_points(params["points"]).ok_or_else(|| DemoError::bad_param("points", params["points"]))?;
    let clock = t + host.ahead;

    let history = (0..points).rev().map(|back| cpu(host, t - back * 5)).collect();
    let memory = round1(host.memory + 19.0 * wave(clock, 600) + 3.0 * (noise(host.seed + 10, t / 5) - 0.5));

    // The disk fills up over ten minutes, then a made-up clean-up empties it.
    let filled = clock.rem_euclid(600) as f64 / 600.0;
    let disk_free = round1(host.disk.0 - (host.disk.0 - host.disk.1) * filled);

    // Every two minutes the backup goes ok, then late, then failed.
    let backup = match clock.rem_euclid(120) {
        0..80 => Status::Ok,
        80..105 => Status::Warn,
        _ => Status::Crit,
    };

    let mail = if (100..130).contains(&clock.rem_euclid(180)) {
        Item::new("mail").text("restarting").status(Status::Warn)
    } else {
        Item::new("mail").status(Status::Ok)
    };
    let scheduler = if (300..345).contains(&clock.rem_euclid(420)) {
        Item::new("scheduler").text("stopped").status(Status::Crit)
    } else {
        Item::new("scheduler").status(Status::Ok)
    };
    let services = vec![Item::new("web").status(Status::Ok), Item::new("database").status(Status::Ok), mail, scheduler];

    let up_for = (t + host.uptime.0).rem_euclid(host.uptime.1 * 86_400);

    Ok(vec![
        ("cpu", Entry::number(cpu(host, t))),
        ("cpu_history", Entry::series(history)),
        ("memory", Entry::number(memory)),
        ("disk_free", Entry::number(disk_free)),
        ("uptime", Entry::text(uptime(up_for))),
        ("backup", Entry::status(backup)),
        ("services", Entry::list(services)),
    ])
}

fn uptime(secs: i64) -> String {
    let (days, hours, minutes) = (secs / 86_400, secs % 86_400 / 3600, secs % 3600 / 60);
    if days > 0 { format!("{days}d {hours}h") } else { format!("{hours}h {minutes}m") }
}

struct Place {
    seed: u64,
    temp: f64,
    wind: f64,
}

const NORTHAVEN: Place = Place { seed: 11, temp: 8.0, wind: 20.0 };
const SALTMERE: Place = Place { seed: 12, temp: 16.0, wind: 26.0 };
const EMBERFALL: Place = Place { seed: 13, temp: 27.0, wind: 48.0 };

const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/// The temperature in a made-up hour: warmest mid-afternoon, and each made-up day a bit different.
fn temp(place: &Place, hour: i64) -> f64 {
    let time_of_day = ((hour.rem_euclid(24) - 9) as f64 / 24.0 * TAU).sin();
    let this_day = 3.0 * (noise(place.seed + 1, hour.div_euclid(24)) - 0.5);
    round1(place.temp + 6.0 * time_of_day + this_day + 1.5 * (noise(place.seed, hour) - 0.5))
}

fn weather(t: i64, params: &Params) -> Values {
    let place = match params["place"] {
        "saltmere" => &SALTMERE,
        "emberfall" => &EMBERFALL,
        _ => &NORTHAVEN,
    };
    let hour = t.div_euclid(60); // one made-up hour per minute
    let day = hour.div_euclid(24);

    let now = temp(place, hour);
    let wind = round1((place.wind + 18.0 * wave(t, 1020) + 14.0 * (noise(place.seed + 2, hour) - 0.5)).max(0.0));
    let rain = (50.0 + 45.0 * wave(t + place.seed as i64 * 400, 1860) + 10.0 * (noise(place.seed + 3, hour) - 0.5))
        .clamp(0.0, 100.0)
        .round();
    let condition = match rain as i64 {
        75.. => "Rain",
        55..75 => "Showers",
        _ if wind >= 40.0 => "Windy",
        35..55 => "Cloudy",
        15..35 => "Partly cloudy",
        _ => "Clear",
    };

    let weekday = t.div_euclid(86_400) + 4; // 1970-01-01 was a Thursday
    let forecast = (0..5)
        .map(|ahead| {
            let label = if ahead == 0 { "Today" } else { WEEKDAYS[(weekday + ahead).rem_euclid(7) as usize] };
            let high = round1(place.temp + 6.0 + 3.0 * (noise(place.seed + 1, day + ahead) - 0.5));
            Item::new(label).number(high)
        })
        .collect();

    vec![
        ("temp", Entry::number(now)),
        ("feels_like", Entry::number(round1(now - wind / 12.0))),
        ("condition", Entry::text(condition)),
        ("wind", Entry::number(wind)),
        ("rain_chance", Entry::number(rain)),
        ("temp_24h", Entry::series((0..24).rev().map(|back| temp(place, hour - back)).collect())),
        ("forecast", Entry::list(forecast)),
    ]
}

/// Category names with their usual share of the spending.
const CATEGORIES: [(&str, f64); 5] =
    [("Groceries", 34.0), ("Bills", 24.0), ("Eating out", 18.0), ("Transport", 14.0), ("Fun", 10.0)];

/// The money spent on one made-up day: $35 to $135.
fn spent_on(day: i64) -> f64 {
    35.0 + 100.0 * noise(21, day)
}

fn spending(t: i64, params: &Params) -> Values {
    let (days, budget) = if params["period"] == "month" { (30, 2550.0) } else { (7, 600.0) };
    let day = t.div_euclid(60); // one made-up day per minute
    let part_of_today = t.rem_euclid(60) as f64 / 60.0;
    let today = spent_on(day) * part_of_today;

    let start = day - day.rem_euclid(days);
    let total = (start..day).map(spent_on).sum::<f64>() + today;

    // Over budget is bad; spending clearly faster than the budget allows is a warning.
    let days_in = (day - start) as f64 + part_of_today;
    let state = if total > budget {
        Status::Crit
    } else if days_in >= 1.0 && total > 1.1 * budget * days_in / days as f64 {
        Status::Warn
    } else {
        Status::Ok
    };

    let daily = (0..days).rev().map(|back| round2(if back == 0 { today } else { spent_on(day - back) })).collect();

    // Each period splits the money a little differently.
    let period = start.div_euclid(days);
    let weights: Vec<f64> =
        (0u64..).zip(CATEGORIES).map(|(i, (_, share))| share * (0.6 + 0.8 * noise(30 + i, period))).collect();
    let all: f64 = weights.iter().sum();
    let mut categories: Vec<(&str, f64)> =
        CATEGORIES.iter().zip(&weights).map(|((name, _), weight)| (*name, round2(total * weight / all))).collect();
    categories.sort_by(|a, b| b.1.total_cmp(&a.1));

    // Nothing spent yet at the very start of a period: no biggest category.
    let biggest = if total > 0.0 { Entry::text(categories[0].0) } else { Entry::none() };
    let top = categories.into_iter().map(|(name, amount)| Item::new(name).number(amount)).collect();

    vec![
        ("total", Entry::number(round2(total))),
        ("budget_left", Entry::number(round2(budget - total))),
        ("daily", Entry::series(daily)),
        ("top_categories", Entry::list(top)),
        ("biggest", biggest),
        ("state", Entry::status(state)),
    ]
}

/// A smooth swing between -1 and 1 that repeats every `period` seconds.
fn wave(t: i64, period: i64) -> f64 {
    (t.rem_euclid(period) as f64 / period as f64 * TAU).sin()
}

/// A made-up number from 0 up to (not including) 1. The same inputs always give the same number.
fn noise(seed: u64, n: i64) -> f64 {
    // splitmix64, a well-known public-domain bit mixer.
    let mut x = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (n as u64).wrapping_mul(0xD6E8_FEB8_6659_FD93);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    (x >> 11) as f64 / (1u64 << 53) as f64
}

// `+ 0.0` turns a negative zero into a plain zero, so "-0.0" is never sent.

fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0 + 0.0
}

fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0 + 0.0
}

// --------------------------------------------------------------------- http

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

fn error(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(ErrorBody { error: message.into() })).into_response()
}

/// The demo app's routes: the two contract endpoints and a small page for people.
pub fn router() -> Router {
    Router::new()
        .route("/", get(home))
        .route("/widgets", get(get_catalogue))
        .route("/widgets/{source}", get(get_data))
        .fallback(|| async { error(StatusCode::NOT_FOUND, "nothing here; try /widgets") })
}

/// Serves the demo app on `listener` until the process stops.
pub async fn serve(listener: tokio::net::TcpListener) -> std::io::Result<()> {
    axum::serve(listener, router()).await
}

async fn get_catalogue() -> Json<&'static Catalogue> {
    Json(catalogue())
}

async fn get_data(Path(source): Path<String>, Query(query): Query<HashMap<String, String>>) -> Response {
    match data(&source, &query, rfc3339::now()) {
        Ok(data) => Json(data).into_response(),
        Err(e) => error(e.status(), e.to_string()),
    }
}

async fn home() -> Html<&'static str> {
    Html(&HOME)
}

/// The page at `/`. Each source has an anchor, which is where the data's `link` points.
static HOME: LazyLock<String> = LazyLock::new(|| {
    let mut page = String::from(
        "<!doctype html>\n<html lang=\"en\">\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>tileboard demo app</title>\n\
         <style>body{font:16px/1.5 system-ui,sans-serif;max-width:42rem;margin:2rem auto;padding:0 1rem}</style>\n\
         <h1>tileboard demo app</h1>\n\
         <p>Everything here is made up. This app exists so a tileboard dashboard has something to show.</p>\n\
         <p>The catalogue: <a href=\"/widgets\">/widgets</a></p>\n",
    );
    for source in &CATALOGUE.sources {
        let fields: Vec<String> =
            source.fields.iter().map(|f| format!("{} ({})", escape(&f.label), f.kind.as_str())).collect();
        page.push_str(&format!(
            "<h2 id=\"{id}\">{title}</h2>\n\
             <p><a href=\"/widgets/{id}\">/widgets/{id}</a>, new values every {refresh} s</p>\n\
             <p>{fields}</p>\n",
            id = source.id,
            title = escape(&source.title),
            refresh = source.refresh_s,
            fields = fields.join(", "),
        ));
    }
    page.push_str("</html>\n");
    page
});

fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::contract::Value;

    /// 2026-11-24T00:00:00Z
    const T0: i64 = 1_795_478_400;
    const NO_PROBLEMS: Vec<String> = Vec::new();

    fn query(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(name, value)| (name.to_string(), value.to_string())).collect()
    }

    /// Every combination of the source's choice params.
    fn every_choice(source: &Source) -> Vec<HashMap<String, String>> {
        let mut combos = vec![HashMap::new()];
        for param in source.params.iter().filter(|p| p.kind == ParamType::Choice) {
            combos = combos
                .iter()
                .flat_map(|combo| {
                    param.options.iter().map(move |option| {
                        let mut combo = combo.clone();
                        combo.insert(param.name.clone(), option.value.clone());
                        combo
                    })
                })
                .collect();
        }
        combos
    }

    fn number(data: &Data, field: &str) -> f64 {
        match data.values[field].value {
            Value::Number(n) => n,
            ref other => panic!("{field} is {other:?}, not a number"),
        }
    }

    /// Every status `field` takes when `source` is asked every `step` seconds for `seconds`.
    fn statuses(source: &str, pairs: &[(&str, &str)], field: &str, seconds: i64, step: usize) -> HashSet<Status> {
        let spec = catalogue().source(source).unwrap().field(field).unwrap();
        (T0..T0 + seconds)
            .step_by(step)
            .filter_map(|t| spec.status_of(&data(source, &query(pairs), t).unwrap().values[field]))
            .collect()
    }

    const EVERY_STATUS: [Status; 3] = [Status::Ok, Status::Warn, Status::Crit];

    #[test]
    fn the_catalogue_follows_the_spec() {
        assert_eq!(catalogue().problems(), NO_PROBLEMS);
    }

    #[test]
    fn every_answer_follows_the_catalogue() {
        for source in &catalogue().sources {
            for choice in every_choice(source) {
                for t in (T0..T0 + 3600).step_by(7) {
                    let answer = data(&source.id, &choice, t).unwrap();
                    assert_eq!(source.data_problems(&answer), NO_PROBLEMS, "{} {choice:?} at {t}", source.id);
                }
            }
        }
    }

    #[test]
    fn the_same_moment_gives_the_same_answer() {
        for source in &catalogue().sources {
            let none = HashMap::new();
            assert_eq!(data(&source.id, &none, T0 + 1234), data(&source.id, &none, T0 + 1234));
        }
    }

    #[test]
    fn answers_change_once_per_refresh() {
        for source in &catalogue().sources {
            let none = HashMap::new();
            let refresh = source.refresh_s as i64;
            let at = |t| data(&source.id, &none, t).unwrap();

            // T0 is midnight, so it starts a refresh for every source.
            assert_eq!(at(T0).updated_at, rfc3339::format(T0));
            assert_eq!(at(T0), at(T0 + refresh - 1), "{}", source.id);
            assert_eq!(at(T0 + refresh).updated_at, rfc3339::format(T0 + refresh));

            let different: HashSet<String> =
                (0..20).map(|n| serde_json::to_string(&at(T0 + n * refresh).values).unwrap()).collect();
            assert!(different.len() >= 15, "{} barely moves: {} different answers in 20", source.id, different.len());
        }
    }

    #[test]
    fn left_out_params_use_their_defaults() {
        let none = HashMap::new();
        assert_eq!(data("server", &none, T0), data("server", &query(&[("host", "atlas"), ("points", "30")]), T0));
        assert_eq!(data("weather", &none, T0), data("weather", &query(&[("place", "northaven")]), T0));
        assert_eq!(data("spending", &none, T0), data("spending", &query(&[("period", "week")]), T0));
    }

    #[test]
    fn query_params_the_source_does_not_have_are_ignored() {
        assert_eq!(
            data("weather", &query(&[("host", "zzz"), ("colour", "red")]), T0),
            data("weather", &HashMap::new(), T0)
        );
    }

    #[test]
    fn params_change_the_answer() {
        let at = |source, pairs: &[(&str, &str)]| data(source, &query(pairs), T0 + 600).unwrap().values;
        assert_ne!(at("server", &[("host", "atlas")]), at("server", &[("host", "comet")]));
        assert_ne!(at("weather", &[("place", "northaven")]), at("weather", &[("place", "emberfall")]));
        assert_ne!(at("spending", &[("period", "week")]), at("spending", &[("period", "month")]));
    }

    #[test]
    fn bad_param_values_are_refused() {
        let refused = |source, name: &str, value: &str| {
            assert_eq!(
                data(source, &query(&[(name, value)]), T0),
                Err(DemoError::bad_param(name, value)),
                "{source} {name}={value:?}"
            );
        };
        refused("server", "host", "zzz");
        refused("server", "host", "");
        refused("server", "host", "Atlas");
        for points in ["4", "121", "7.5", "-30", "many", "", "inf", "NaN"] {
            refused("server", "points", points);
        }
        refused("weather", "place", "atlas");
        refused("spending", "period", "year");
    }

    #[test]
    fn an_unknown_source_is_refused() {
        for id in ["nope", "", "Server", "server/extra"] {
            assert_eq!(data(id, &HashMap::new(), T0), Err(DemoError::UnknownSource(id.to_string())));
        }
    }

    #[test]
    fn errors_map_to_the_codes_in_the_spec() {
        assert_eq!(DemoError::UnknownSource("x".into()).status(), StatusCode::NOT_FOUND);
        assert_eq!(DemoError::bad_param("host", "x").status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn points_sets_the_history_length() {
        for (points, expected) in [(None, 30), (Some("5"), 5), (Some("120"), 120), (Some("12.0"), 12)] {
            let query = points.map_or_else(HashMap::new, |p| query(&[("points", p)]));
            let answer = data("server", &query, T0).unwrap();
            assert_eq!(answer.values["cpu_history"].series.as_ref().unwrap().len(), expected);
        }
    }

    #[test]
    fn a_series_ends_with_the_current_value() {
        for source in &catalogue().sources {
            let answer = data(&source.id, &HashMap::new(), T0 + 777).unwrap();
            for field in source.fields.iter().filter(|f| f.kind == FieldType::Series) {
                let entry = &answer.values[&field.id];
                let last = entry.series.as_ref().unwrap().last().copied().flatten().unwrap();
                assert_eq!(entry.value, Value::Number(last), "{}.{}", source.id, field.id);
            }
        }
    }

    #[test]
    fn the_history_is_what_was_shown_before() {
        let none = HashMap::new();
        let now = data("server", &none, T0 + 500).unwrap();
        let history = now.values["cpu_history"].series.as_ref().unwrap();
        assert_eq!(number(&now, "cpu"), history[29].unwrap());
        for back in 1..30 {
            let then = data("server", &none, T0 + 500 - back * 5).unwrap();
            assert_eq!(Some(number(&then, "cpu")), history[29 - back as usize]);
        }
    }

    #[test]
    fn the_backup_status_flips() {
        assert_eq!(statuses("server", &[], "backup", 120, 5), HashSet::from(EVERY_STATUS));
        assert_eq!(statuses("server", &[("host", "comet")], "backup", 120, 5), HashSet::from(EVERY_STATUS));
    }

    #[test]
    fn thresholds_are_crossed_in_both_directions() {
        // Higher is worse.
        assert_eq!(statuses("server", &[], "cpu", 7200, 5), HashSet::from(EVERY_STATUS));
        // Lower is worse.
        assert_eq!(statuses("server", &[], "disk_free", 600, 5), HashSet::from(EVERY_STATUS));
        assert_eq!(statuses("spending", &[], "budget_left", 7200, 30), HashSet::from(EVERY_STATUS));
        // The quiet host stays fine.
        assert_eq!(statuses("server", &[("host", "comet")], "cpu", 7200, 5), HashSet::from([Status::Ok]));
        assert_eq!(statuses("server", &[("host", "comet")], "disk_free", 600, 5), HashSet::from([Status::Ok]));
    }

    #[test]
    fn spending_adds_up() {
        for (period, budget) in [("week", 600.0), ("month", 2550.0)] {
            for t in (T0..T0 + 3600).step_by(30) {
                let answer = data("spending", &query(&[("period", period)]), t).unwrap();
                let total = number(&answer, "total");
                assert!(total >= 0.0);
                assert!((total + number(&answer, "budget_left") - budget).abs() < 0.011, "{period} at {t}");

                let Value::List(categories) = &answer.values["top_categories"].value else {
                    panic!("top_categories is not a list");
                };
                let amounts: Vec<f64> = categories
                    .iter()
                    .map(|item| match item.value {
                        Some(crate::contract::ItemValue::Number(n)) => n,
                        ref other => panic!("category amount is {other:?}"),
                    })
                    .collect();
                assert!((amounts.iter().sum::<f64>() - total).abs() < 0.03, "{period} at {t}");
                assert!(amounts.is_sorted_by(|a, b| a >= b), "biggest first: {amounts:?}");

                match &answer.values["biggest"].value {
                    Value::Null => assert_eq!(total, 0.0),
                    Value::Text(name) => assert_eq!(name, &categories[0].label),
                    other => panic!("biggest is {other:?}"),
                }
            }
        }
    }

    #[test]
    fn a_new_period_starts_with_nothing_spent() {
        // Made-up days are minutes; day 0 started at Unix time 0, so weeks start every 420 s.
        let week_start = T0 - T0.rem_euclid(420) + 420;
        let answer = data("spending", &HashMap::new(), week_start).unwrap();
        assert_eq!(number(&answer, "total"), 0.0);
        assert_eq!(answer.values["biggest"].value, Value::Null);
        assert_eq!(answer.values["state"], Entry::status(Status::Ok));
    }

    #[test]
    fn the_home_page_has_an_anchor_for_every_link() {
        for source in &catalogue().sources {
            let answer = data(&source.id, &HashMap::new(), T0).unwrap();
            assert_eq!(answer.link, Some(format!("/#{}", source.id)));
            assert!(HOME.contains(&format!("id=\"{}\"", source.id)));
        }
    }

    #[test]
    fn text_is_escaped_on_the_home_page() {
        assert_eq!(escape(r#"<a href="x">Tom & Jerry</a>"#), "&lt;a href=&quot;x&quot;&gt;Tom &amp; Jerry&lt;/a&gt;");
    }

    #[test]
    fn the_starter_layout_only_uses_what_the_catalogue_has() {
        let layout = starter_layout("demo");
        assert_eq!(layout.problems(), NO_PROBLEMS);
        assert_eq!(layout.rev, 0);
        for tile in &layout.tiles {
            for field in &tile.fields {
                assert_eq!(field.app, "demo");
                let source = catalogue().source(&field.source).expect("a source of the demo app");
                assert!(source.field(&field.field).is_some(), "{}: {}.{}", tile.label, field.source, field.field);
                for (name, value) in &field.params {
                    assert!(source.param(name).expect("a param of the source").accepts(value), "{name}={value}");
                }
            }
        }
        // Every view and every source of the demo app is on show.
        let views: HashSet<String> = layout.tiles.iter().map(|tile| format!("{:?}", tile.view)).collect();
        assert_eq!(views.len(), 5);
        let sources: HashSet<&str> = layout.tiles.iter().map(|tile| tile.fields[0].source.as_str()).collect();
        assert_eq!(sources.len(), catalogue().sources.len());
    }

    #[test]
    fn starter_tiles_do_not_overlap() {
        let tiles = starter_layout("demo").tiles;
        for (i, a) in tiles.iter().enumerate() {
            for b in &tiles[i + 1..] {
                let apart = a.x + a.w <= b.x || b.x + b.w <= a.x || a.y + a.h <= b.y || b.y + b.h <= a.y;
                assert!(apart, "{} and {} overlap", a.label, b.label);
            }
        }
    }
}
