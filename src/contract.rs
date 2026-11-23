//! Widget contract v2 as Rust types, plus the checks a JSON Schema cannot do.
//!
//! `SPEC.md` is the source of truth. Reading is forgiving (unknown properties
//! are ignored, as the spec asks of dashboards); the `problems` checks are strict.

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::rfc3339;

/// The contract version this code speaks.
pub const CONTRACT: u32 = 2;

/// The answer to `GET /widgets`: what data an app has.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Catalogue {
    pub contract: u32,
    pub app: String,
    pub title: String,
    pub sources: Vec<Source>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub presets: Vec<Preset>,
}

/// A group of fields returned by one fetch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Source {
    pub id: String,
    pub title: String,
    pub refresh_s: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<Param>,
    pub fields: Vec<Field>,
}

/// A choice that changes what a source returns.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Param {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(rename = "type")]
    pub kind: ParamType,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<ParamOption>,
    pub default: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ParamType {
    Choice,
    Text,
    Number,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParamOption {
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// One data point.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Field {
    pub id: String,
    pub label: String,
    #[serde(rename = "type")]
    pub kind: FieldType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warn: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crit: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FieldType {
    Number,
    Text,
    Status,
    Series,
    List,
}

impl FieldType {
    pub fn as_str(self) -> &'static str {
        match self {
            FieldType::Number => "number",
            FieldType::Text => "text",
            FieldType::Status => "status",
            FieldType::Series => "series",
            FieldType::List => "list",
        }
    }
}

/// A tile the app suggests. Only a starting point.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Preset {
    pub title: String,
    pub source: String,
    pub fields: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view: Option<View>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum View {
    Number,
    List,
    Sparkline,
    Table,
    Status,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Ok,
    Warn,
    Crit,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Ok => "ok",
            Status::Warn => "warn",
            Status::Crit => "crit",
        }
    }

    pub fn parse(s: &str) -> Option<Status> {
        match s {
            "ok" => Some(Status::Ok),
            "warn" => Some(Status::Warn),
            "crit" => Some(Status::Crit),
            _ => None,
        }
    }
}

/// The answer to `GET /widgets/<source>`: the current values of one source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Data {
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
    pub values: BTreeMap<String, Entry>,
}

/// The current value of one field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub value: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<Status>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub series: Option<Vec<Option<f64>>>,
}

/// What `value` can hold. Which one is right depends on the field's type;
/// a `status` field carries its status as `Text`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Value {
    /// No data right now.
    Null,
    Number(f64),
    Text(String),
    List(Vec<Item>),
}

/// One line of a `list` field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<ItemValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<Status>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ItemValue {
    Number(f64),
    Text(String),
}

impl Entry {
    fn of(value: Value) -> Entry {
        Entry {
            value,
            status: None,
            series: None,
        }
    }

    /// No data right now.
    pub fn none() -> Entry {
        Entry::of(Value::Null)
    }

    pub fn number(value: f64) -> Entry {
        Entry::of(Value::Number(value))
    }

    pub fn text(value: impl Into<String>) -> Entry {
        Entry::of(Value::Text(value.into()))
    }

    /// The entry of a `status` field.
    pub fn status(status: Status) -> Entry {
        Entry::of(Value::Text(status.as_str().to_string()))
    }

    /// The entry of a `series` field: oldest point first, the last point is the current value.
    pub fn series(points: Vec<f64>) -> Entry {
        Entry {
            value: points.last().map_or(Value::Null, |last| Value::Number(*last)),
            status: None,
            series: Some(points.into_iter().map(Some).collect()),
        }
    }

    pub fn list(items: Vec<Item>) -> Entry {
        Entry::of(Value::List(items))
    }

    /// Adds a status that wins over the field's thresholds.
    pub fn with_status(mut self, status: Status) -> Entry {
        self.status = Some(status);
        self
    }
}

impl Item {
    pub fn new(label: impl Into<String>) -> Item {
        Item {
            label: label.into(),
            value: None,
            status: None,
        }
    }

    pub fn number(mut self, value: f64) -> Item {
        self.value = Some(ItemValue::Number(value));
        self
    }

    pub fn text(mut self, value: impl Into<String>) -> Item {
        self.value = Some(ItemValue::Text(value.into()));
        self
    }

    pub fn status(mut self, status: Status) -> Item {
        self.status = Some(status);
        self
    }
}

/// Lowercase letters, digits, `_` and `-`, starting with a letter or digit, 1 to 64 long.
pub fn is_id(s: &str) -> bool {
    let plain = |c: u8| c.is_ascii_lowercase() || c.is_ascii_digit();
    let bytes = s.as_bytes();
    (1..=64).contains(&bytes.len()) && plain(bytes[0]) && bytes.iter().all(|&c| plain(c) || c == b'_' || c == b'-')
}

/// A full `http://` / `https://` address, or a path starting with exactly one `/`.
pub fn is_link(s: &str) -> bool {
    match s.strip_prefix("http://").or_else(|| s.strip_prefix("https://")) {
        Some(rest) => !rest.is_empty(),
        None => s.starts_with('/') && !s.starts_with("//"),
    }
}

impl Catalogue {
    pub fn source(&self, id: &str) -> Option<&Source> {
        self.sources.iter().find(|s| s.id == id)
    }

    /// Everything wrong with this catalogue, in words. Empty means it follows the spec.
    pub fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.contract != CONTRACT {
            out.push(format!("contract is {}, expected {CONTRACT}", self.contract));
        }
        if !is_id(&self.app) {
            out.push(format!("app {:?} is not a valid id", self.app));
        }
        if self.title.is_empty() {
            out.push("title is empty".to_string());
        }
        if self.sources.is_empty() {
            out.push("there are no sources".to_string());
        }

        let mut seen = HashSet::new();
        for source in &self.sources {
            if !seen.insert(&source.id) {
                out.push(format!("source id {:?} is used twice", source.id));
            }
            source.collect_problems(&mut out);
        }

        for preset in &self.presets {
            let at = format!("preset {:?}", preset.title);
            if preset.title.is_empty() {
                out.push(format!("{at}: title is empty"));
            }
            let Some(source) = self.source(&preset.source) else {
                out.push(format!("{at}: source {:?} does not exist", preset.source));
                continue;
            };
            if preset.fields.is_empty() {
                out.push(format!("{at}: there are no fields"));
            }
            let mut seen = HashSet::new();
            for id in &preset.fields {
                if source.field(id).is_none() {
                    out.push(format!("{at}: field {id:?} is not in source {:?}", source.id));
                }
                if !seen.insert(id) {
                    out.push(format!("{at}: field {id:?} is listed twice"));
                }
            }
            for (name, value) in &preset.params {
                match source.param(name) {
                    None => {
                        out.push(format!("{at}: param {name:?} is not in source {:?}", source.id));
                    }
                    Some(param) if !param.accepts(value) => {
                        out.push(format!("{at}: {value:?} is not a valid value for param {name:?}"));
                    }
                    Some(_) => {}
                }
            }
        }
        out
    }
}

impl Source {
    pub fn field(&self, id: &str) -> Option<&Field> {
        self.fields.iter().find(|f| f.id == id)
    }

    pub fn param(&self, name: &str) -> Option<&Param> {
        self.params.iter().find(|p| p.name == name)
    }

    fn collect_problems(&self, out: &mut Vec<String>) {
        let at = format!("source {:?}", self.id);
        if !is_id(&self.id) {
            out.push(format!("{at}: id is not a valid id"));
        }
        if self.title.is_empty() {
            out.push(format!("{at}: title is empty"));
        }
        if self.refresh_s == 0 {
            out.push(format!("{at}: refresh_s must be 1 or more"));
        }
        if self.fields.is_empty() {
            out.push(format!("{at}: there are no fields"));
        }

        let mut names = HashSet::new();
        for param in &self.params {
            let at = format!("{at}, param {:?}", param.name);
            if !is_id(&param.name) {
                out.push(format!("{at}: name is not a valid id"));
            }
            if !names.insert(&param.name) {
                out.push(format!("{at}: name is used twice"));
            }
            if param.label.as_deref() == Some("") {
                out.push(format!("{at}: label is empty"));
            }
            if param.kind == ParamType::Choice {
                if param.options.is_empty() {
                    out.push(format!("{at}: a choice needs options"));
                }
                let mut values = HashSet::new();
                for option in &param.options {
                    if !values.insert(&option.value) {
                        out.push(format!("{at}: option value {:?} is used twice", option.value));
                    }
                    if option.label.as_deref() == Some("") {
                        out.push(format!("{at}: option {:?} has an empty label", option.value));
                    }
                }
            } else if !param.options.is_empty() {
                out.push(format!("{at}: only a choice has options"));
            }
            if !param.accepts(&param.default) {
                out.push(format!("{at}: default {:?} is not a valid value", param.default));
            }
        }

        let mut ids = HashSet::new();
        for field in &self.fields {
            let at = format!("{at}, field {:?}", field.id);
            if !is_id(&field.id) {
                out.push(format!("{at}: id is not a valid id"));
            }
            if !ids.insert(&field.id) {
                out.push(format!("{at}: id is used twice"));
            }
            if field.label.is_empty() {
                out.push(format!("{at}: label is empty"));
            }
            let measured = matches!(field.kind, FieldType::Number | FieldType::Series);
            if (field.warn.is_some() || field.crit.is_some()) && !measured {
                out.push(format!("{at}: warn and crit are only for number and series fields"));
            }
            if field.unit.is_some() && !measured && field.kind != FieldType::List {
                out.push(format!("{at}: unit is only for number, series and list fields"));
            }
            if field.warn.is_some() && field.warn == field.crit {
                out.push(format!("{at}: warn and crit must differ"));
            }
        }
    }

    /// Everything wrong with a data answer for this source. Empty means it follows the spec.
    pub fn data_problems(&self, data: &Data) -> Vec<String> {
        let mut out = Vec::new();
        if rfc3339::parse(&data.updated_at).is_none() {
            out.push(format!("updated_at {:?} is not an RFC 3339 date-time", data.updated_at));
        }
        if let Some(link) = &data.link
            && !is_link(link)
        {
            out.push(format!("link {link:?} is neither an http(s) address nor a path"));
        }
        for field in &self.fields {
            match data.values.get(&field.id) {
                None => out.push(format!("field {:?} is missing", field.id)),
                Some(entry) => {
                    if let Err(problem) = field.check_entry(entry) {
                        out.push(format!("field {:?}: {problem}", field.id));
                    }
                }
            }
        }
        out
    }
}

impl Param {
    /// Whether `value` is allowed for this param.
    pub fn accepts(&self, value: &str) -> bool {
        match self.kind {
            ParamType::Choice => self.options.iter().any(|o| o.value == value),
            ParamType::Number => value.parse::<f64>().is_ok_and(f64::is_finite),
            ParamType::Text => true,
        }
    }
}

impl Field {
    /// Whether `entry` is the right shape for this field's type.
    pub fn check_entry(&self, entry: &Entry) -> Result<(), String> {
        let value_fits = match (self.kind, &entry.value) {
            (_, Value::Null) => true,
            (FieldType::Number | FieldType::Series, Value::Number(n)) => n.is_finite(),
            (FieldType::Text, Value::Text(_)) => true,
            (FieldType::Status, Value::Text(s)) => Status::parse(s).is_some(),
            (FieldType::List, Value::List(_)) => true,
            _ => false,
        };
        if !value_fits {
            return Err(format!("value does not fit a {} field", self.kind.as_str()));
        }
        match (self.kind, &entry.series) {
            (FieldType::Series, None) => Err("a series field needs `series`".to_string()),
            (FieldType::Series, Some(points)) if points.iter().flatten().any(|p| !p.is_finite()) => {
                Err("`series` holds a number that is not finite".to_string())
            }
            (FieldType::Series, Some(_)) | (_, None) => Ok(()),
            (_, Some(_)) => Err("only a series field has `series`".to_string()),
        }
    }

    /// The status of `entry`, worked out as "Status and thresholds" in SPEC.md says.
    pub fn status_of(&self, entry: &Entry) -> Option<Status> {
        if self.kind == FieldType::Status {
            return match &entry.value {
                Value::Text(s) => Status::parse(s),
                _ => None,
            };
        }
        if entry.status.is_some() {
            return entry.status;
        }
        if !matches!(self.kind, FieldType::Number | FieldType::Series) {
            return None;
        }
        let Value::Number(v) = entry.value else {
            return None;
        };
        let status = match (self.warn, self.crit) {
            (None, None) => return None,
            // Lower is worse.
            (Some(warn), Some(crit)) if crit < warn => {
                if v <= crit {
                    Status::Crit
                } else if v <= warn {
                    Status::Warn
                } else {
                    Status::Ok
                }
            }
            // Higher is worse.
            (warn, crit) => {
                if crit.is_some_and(|crit| v >= crit) {
                    Status::Crit
                } else if warn.is_some_and(|warn| v >= warn) {
                    Status::Warn
                } else {
                    Status::Ok
                }
            }
        };
        Some(status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(kind: FieldType, warn: Option<f64>, crit: Option<f64>) -> Field {
        Field {
            id: "f".into(),
            label: "F".into(),
            kind,
            unit: None,
            warn,
            crit,
        }
    }

    fn small_catalogue() -> Catalogue {
        serde_json::from_value(serde_json::json!({
            "contract": 2,
            "app": "shop",
            "title": "Shop",
            "sources": [{
                "id": "orders",
                "title": "Orders",
                "refresh_s": 60,
                "params": [
                    {"name": "period", "type": "choice", "options": [{"value": "day"}, {"value": "week"}], "default": "day"},
                    {"name": "top", "type": "number", "default": "5"}
                ],
                "fields": [
                    {"id": "count", "label": "Orders", "type": "number", "warn": 100, "crit": 200},
                    {"id": "trend", "label": "Trend", "type": "series"},
                    {"id": "state", "label": "State", "type": "status"},
                    {"id": "note", "label": "Note", "type": "text"},
                    {"id": "latest", "label": "Latest", "type": "list", "unit": "$"}
                ]
            }],
            "presets": [
                {"title": "Orders this week", "source": "orders", "fields": ["count", "trend"], "params": {"period": "week"}, "view": "table"}
            ]
        }))
        .unwrap()
    }

    /// Applies `change` to a good catalogue and returns what is now wrong with it.
    fn broken(change: impl FnOnce(&mut Catalogue)) -> Vec<String> {
        let mut catalogue = small_catalogue();
        change(&mut catalogue);
        catalogue.problems()
    }

    #[test]
    fn ids() {
        let (longest, too_long) = ("a".repeat(64), "a".repeat(65));
        for good in ["a", "0", "cpu", "cpu_history", "disk-free", "a1_b-2", longest.as_str()] {
            assert!(is_id(good), "{good:?}");
        }
        for bad in ["", "_a", "-a", "CPU", "a b", "a.b", "a/b", "é", too_long.as_str()] {
            assert!(!is_id(bad), "{bad:?}");
        }
    }

    #[test]
    fn links() {
        for good in [
            "/",
            "/servers/atlas",
            "/#server",
            "/?a=1",
            "http://app.example/x",
            "https://app.example",
        ] {
            assert!(is_link(good), "{good:?}");
        }
        for bad in [
            "",
            "servers",
            "//other.example/x",
            "http://",
            "https://",
            "ftp://app.example",
            "javascript:alert(1)",
        ] {
            assert!(!is_link(bad), "{bad:?}");
        }
    }

    #[test]
    fn a_good_catalogue_has_no_problems() {
        assert_eq!(small_catalogue().problems(), Vec::<String>::new());
    }

    #[test]
    fn catalogue_problems_are_found() {
        type Change = Box<dyn FnOnce(&mut Catalogue)>;
        let cases: Vec<(&str, Change)> = vec![
            ("contract is 1", Box::new(|c| c.contract = 1)),
            ("not a valid id", Box::new(|c| c.app = "My App".into())),
            ("there are no sources", Box::new(|c| c.sources.clear())),
            ("used twice", Box::new(|c| c.sources.push(c.sources[0].clone()))),
            ("refresh_s must be 1 or more", Box::new(|c| c.sources[0].refresh_s = 0)),
            ("there are no fields", Box::new(|c| c.sources[0].fields.clear())),
            (
                "id is used twice",
                Box::new(|c| c.sources[0].fields[1].id = "count".into()),
            ),
            (
                "name is used twice",
                Box::new(|c| c.sources[0].params[1].name = "period".into()),
            ),
            (
                "a choice needs options",
                Box::new(|c| c.sources[0].params[0].options.clear()),
            ),
            (
                "option value \"day\" is used twice",
                Box::new(|c| c.sources[0].params[0].options[1].value = "day".into()),
            ),
            (
                "default \"year\" is not a valid value",
                Box::new(|c| c.sources[0].params[0].default = "year".into()),
            ),
            (
                "default \"five\" is not a valid value",
                Box::new(|c| c.sources[0].params[1].default = "five".into()),
            ),
            (
                "only a choice has options",
                Box::new(|c| c.sources[0].params[1].options = c.sources[0].params[0].options.clone()),
            ),
            (
                "warn and crit are only for",
                Box::new(|c| c.sources[0].fields[3].warn = Some(1.0)),
            ),
            (
                "unit is only for",
                Box::new(|c| c.sources[0].fields[2].unit = Some("%".into())),
            ),
            (
                "warn and crit must differ",
                Box::new(|c| c.sources[0].fields[0].crit = Some(100.0)),
            ),
            (
                "source \"refunds\" does not exist",
                Box::new(|c| c.presets[0].source = "refunds".into()),
            ),
            (
                "field \"total\" is not in source",
                Box::new(|c| c.presets[0].fields.push("total".into())),
            ),
            (
                "field \"count\" is listed twice",
                Box::new(|c| c.presets[0].fields.push("count".into())),
            ),
            (
                "param \"shop\" is not in source",
                Box::new(|c| {
                    c.presets[0].params.insert("shop".into(), "x".into());
                }),
            ),
            (
                "\"year\" is not a valid value for param \"period\"",
                Box::new(|c| {
                    c.presets[0].params.insert("period".into(), "year".into());
                }),
            ),
        ];
        for (expected, change) in cases {
            let problems = broken(change);
            assert!(
                problems.iter().any(|p| p.contains(expected)),
                "expected a problem containing {expected:?}, got {problems:?}"
            );
        }
    }

    #[test]
    fn unknown_properties_are_ignored_when_reading() {
        let catalogue: Catalogue = serde_json::from_value(serde_json::json!({
            "contract": 2, "app": "a", "title": "A", "from_the_future": true,
            "sources": [{"id": "s", "title": "S", "refresh_s": 1, "colour": "red",
                         "fields": [{"id": "f", "label": "F", "type": "text", "hint": 1}]}]
        }))
        .unwrap();
        assert_eq!(catalogue.problems(), Vec::<String>::new());
    }

    #[test]
    fn data_problems_are_found() {
        let catalogue = small_catalogue();
        let source = &catalogue.sources[0];
        let good = serde_json::json!({
            "updated_at": "2026-11-24T09:30:05Z",
            "link": "/orders",
            "values": {
                "count": {"value": 12},
                "trend": {"value": 3, "series": [1, null, 3]},
                "state": {"value": "ok"},
                "note": {"value": null},
                "latest": {"value": [{"label": "Order 1", "value": 19.9}, {"label": "Order 2", "value": "pending", "status": "warn"}]}
            }
        });
        let problems = |change: &dyn Fn(&mut serde_json::Value)| {
            let mut doc = good.clone();
            change(&mut doc);
            source.data_problems(&serde_json::from_value(doc).unwrap())
        };
        assert_eq!(problems(&|_| {}), Vec::<String>::new());

        type Change = Box<dyn Fn(&mut serde_json::Value)>;
        let cases: Vec<(&str, Change)> = vec![
            (
                "not an RFC 3339 date-time",
                Box::new(|d| d["updated_at"] = "today".into()),
            ),
            (
                "neither an http(s) address nor a path",
                Box::new(|d| d["link"] = "javascript:alert(1)".into()),
            ),
            (
                "field \"count\" is missing",
                Box::new(|d| {
                    d["values"].as_object_mut().unwrap().remove("count");
                }),
            ),
            (
                "field \"count\": value does not fit a number field",
                Box::new(|d| d["values"]["count"]["value"] = "12".into()),
            ),
            (
                "field \"state\": value does not fit a status field",
                Box::new(|d| d["values"]["state"]["value"] = "fine".into()),
            ),
            (
                "field \"note\": value does not fit a text field",
                Box::new(|d| d["values"]["note"]["value"] = 4.into()),
            ),
            (
                "field \"latest\": value does not fit a list field",
                Box::new(|d| d["values"]["latest"]["value"] = "a, b".into()),
            ),
            (
                "field \"trend\": a series field needs `series`",
                Box::new(|d| {
                    d["values"]["trend"].as_object_mut().unwrap().remove("series");
                }),
            ),
            (
                "field \"count\": only a series field has `series`",
                Box::new(|d| d["values"]["count"]["series"] = serde_json::json!([1, 2])),
            ),
        ];
        for (expected, change) in cases {
            let found = problems(&*change);
            assert!(
                found.iter().any(|p| p.contains(expected)),
                "expected a problem containing {expected:?}, got {found:?}"
            );
        }
    }

    #[test]
    fn status_when_higher_is_worse() {
        let cpu = field(FieldType::Number, Some(75.0), Some(90.0));
        assert_eq!(cpu.status_of(&Entry::number(41.5)), Some(Status::Ok));
        assert_eq!(cpu.status_of(&Entry::number(75.0)), Some(Status::Warn));
        assert_eq!(cpu.status_of(&Entry::number(89.9)), Some(Status::Warn));
        assert_eq!(cpu.status_of(&Entry::number(90.0)), Some(Status::Crit));
    }

    #[test]
    fn status_when_lower_is_worse() {
        let disk_free = field(FieldType::Number, Some(50.0), Some(20.0));
        assert_eq!(disk_free.status_of(&Entry::number(120.0)), Some(Status::Ok));
        assert_eq!(disk_free.status_of(&Entry::number(50.0)), Some(Status::Warn));
        assert_eq!(disk_free.status_of(&Entry::number(20.1)), Some(Status::Warn));
        assert_eq!(disk_free.status_of(&Entry::number(18.4)), Some(Status::Crit));
    }

    #[test]
    fn status_with_one_threshold_means_higher_is_worse() {
        let only_warn = field(FieldType::Number, Some(10.0), None);
        assert_eq!(only_warn.status_of(&Entry::number(9.0)), Some(Status::Ok));
        assert_eq!(only_warn.status_of(&Entry::number(10.0)), Some(Status::Warn));
        let only_crit = field(FieldType::Series, None, Some(10.0));
        assert_eq!(only_crit.status_of(&Entry::series(vec![1.0, 9.0])), Some(Status::Ok));
        assert_eq!(only_crit.status_of(&Entry::series(vec![1.0, 11.0])), Some(Status::Crit));
    }

    #[test]
    fn status_sent_by_the_app_wins_over_thresholds() {
        let cpu = field(FieldType::Number, Some(75.0), Some(90.0));
        assert_eq!(
            cpu.status_of(&Entry::number(99.0).with_status(Status::Ok)),
            Some(Status::Ok)
        );
        let note = field(FieldType::Text, None, None);
        assert_eq!(
            note.status_of(&Entry::text("late").with_status(Status::Warn)),
            Some(Status::Warn)
        );
    }

    #[test]
    fn status_of_a_status_field_is_its_value() {
        let backup = field(FieldType::Status, None, None);
        assert_eq!(backup.status_of(&Entry::status(Status::Crit)), Some(Status::Crit));
        assert_eq!(backup.status_of(&Entry::none()), None);
    }

    #[test]
    fn no_thresholds_and_no_data_mean_no_status() {
        assert_eq!(
            field(FieldType::Number, None, None).status_of(&Entry::number(5.0)),
            None
        );
        assert_eq!(
            field(FieldType::Number, Some(1.0), Some(2.0)).status_of(&Entry::none()),
            None
        );
        assert_eq!(
            field(FieldType::Text, None, None).status_of(&Entry::text("hello")),
            None
        );
    }

    #[test]
    fn entries_are_written_the_way_the_spec_shows() {
        let json = |entry: Entry| serde_json::to_string(&entry).unwrap();
        assert_eq!(json(Entry::number(41.5)), r#"{"value":41.5}"#);
        assert_eq!(json(Entry::none()), r#"{"value":null}"#);
        assert_eq!(json(Entry::status(Status::Warn)), r#"{"value":"warn"}"#);
        assert_eq!(
            json(Entry::series(vec![1.5, 2.5])),
            r#"{"value":2.5,"series":[1.5,2.5]}"#
        );
        assert_eq!(
            json(Entry::list(vec![
                Item::new("mail").text("restarting").status(Status::Warn)
            ])),
            r#"{"value":[{"label":"mail","value":"restarting","status":"warn"}]}"#
        );
    }
}
