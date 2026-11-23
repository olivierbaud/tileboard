//! Keeps `SPEC.md`, the JSON Schemas in `schema/` and the demo app in step.
//!
//! The schema checker here knows only the keywords our two schemas use and
//! panics on any other, so a schema cannot quietly outgrow it.

use std::collections::HashMap;
use std::path::Path;

use serde_json::{Value, json};
use tileboard::contract::{self, Catalogue, Data, ParamType, Source, Status};
use tileboard::{demo, rfc3339};

// The two `pattern`s in the schemas. The checker runs our own functions for
// them; the patterns themselves were checked by hand against the same cases
// as the `ids` and `links` unit tests.
const ID_PATTERN: &str = "^[a-z0-9][a-z0-9_-]{0,63}$";
const LINK_PATTERN: &str = "^(https?://.+|/([^/].*)?)$";

fn read(path: &str) -> String {
    let full = Path::new(env!("CARGO_MANIFEST_DIR")).join(path);
    std::fs::read_to_string(&full).unwrap_or_else(|e| panic!("cannot read {}: {e}", full.display()))
}

fn schema(path: &str) -> Value {
    serde_json::from_str(&read(path)).unwrap_or_else(|e| panic!("{path} is not JSON: {e}"))
}

/// What is wrong with `doc` according to `schema`. Empty means it is valid.
fn violations(schema: &Value, doc: &Value) -> Vec<String> {
    let mut out = Vec::new();
    check(schema, schema, doc, "", &mut out);
    out
}

fn is_valid(root: &Value, schema: &Value, doc: &Value) -> bool {
    let mut out = Vec::new();
    check(root, schema, doc, "", &mut out);
    out.is_empty()
}

fn has_type(doc: &Value, name: &str) -> bool {
    match name {
        "object" => doc.is_object(),
        "array" => doc.is_array(),
        "string" => doc.is_string(),
        "number" => doc.is_number(),
        "integer" => doc.as_f64().is_some_and(|n| n.fract() == 0.0),
        "null" => doc.is_null(),
        other => panic!("type {other:?} is not known to the test checker"),
    }
}

fn check(root: &Value, schema: &Value, doc: &Value, at: &str, out: &mut Vec<String>) {
    let at_or_root = if at.is_empty() { "/" } else { at };
    let rules = schema.as_object().expect("a schema is an object");
    for (keyword, rule) in rules {
        match keyword.as_str() {
            "$schema" | "$defs" | "title" | "description" => {}
            "$ref" => {
                let name = rule
                    .as_str()
                    .and_then(|r| r.strip_prefix("#/$defs/"))
                    .expect("a local $ref");
                check(root, &root["$defs"][name], doc, at, out);
            }
            "type" => {
                let fits = match rule {
                    Value::String(name) => has_type(doc, name),
                    Value::Array(names) => names.iter().any(|name| has_type(doc, name.as_str().unwrap())),
                    _ => panic!("type must be a string or a list"),
                };
                if !fits {
                    out.push(format!("{at_or_root}: is not of type {rule}"));
                }
            }
            "const" => {
                if doc != rule {
                    out.push(format!("{at_or_root}: is not {rule}"));
                }
            }
            "enum" => {
                if !rule.as_array().unwrap().contains(doc) {
                    out.push(format!("{at_or_root}: is not one of {rule}"));
                }
            }
            "required" => {
                if let Some(object) = doc.as_object() {
                    for name in rule.as_array().unwrap() {
                        if !object.contains_key(name.as_str().unwrap()) {
                            out.push(format!("{at_or_root}: {name} is missing"));
                        }
                    }
                }
            }
            "properties" => {
                if let Some(object) = doc.as_object() {
                    for (name, sub) in rule.as_object().unwrap() {
                        if let Some(value) = object.get(name) {
                            check(root, sub, value, &format!("{at}/{name}"), out);
                        }
                    }
                }
            }
            "additionalProperties" => {
                if let Some(object) = doc.as_object() {
                    let known = rules.get("properties").and_then(Value::as_object);
                    for (name, value) in object {
                        if known.is_some_and(|known| known.contains_key(name)) {
                            continue;
                        }
                        match rule {
                            Value::Bool(true) => {}
                            Value::Bool(false) => out.push(format!("{at_or_root}: {name:?} is not in the spec")),
                            sub => check(root, sub, value, &format!("{at}/{name}"), out),
                        }
                    }
                }
            }
            "propertyNames" => {
                if let Some(object) = doc.as_object() {
                    for name in object.keys() {
                        check(
                            root,
                            rule,
                            &Value::String(name.clone()),
                            &format!("{at}/{name} (the name)"),
                            out,
                        );
                    }
                }
            }
            "items" => {
                if let Some(items) = doc.as_array() {
                    for (i, item) in items.iter().enumerate() {
                        check(root, rule, item, &format!("{at}/{i}"), out);
                    }
                }
            }
            "minItems" => {
                if doc
                    .as_array()
                    .is_some_and(|items| (items.len() as u64) < rule.as_u64().unwrap())
                {
                    out.push(format!("{at_or_root}: has fewer than {rule} items"));
                }
            }
            "minLength" => {
                if doc
                    .as_str()
                    .is_some_and(|s| (s.chars().count() as u64) < rule.as_u64().unwrap())
                {
                    out.push(format!("{at_or_root}: is shorter than {rule}"));
                }
            }
            "minimum" => {
                if doc.as_f64().is_some_and(|n| n < rule.as_f64().unwrap()) {
                    out.push(format!("{at_or_root}: is less than {rule}"));
                }
            }
            "pattern" => {
                if let Some(text) = doc.as_str() {
                    let fits = match rule.as_str().unwrap() {
                        ID_PATTERN => contract::is_id(text),
                        LINK_PATTERN => contract::is_link(text),
                        other => panic!("pattern {other:?} is not known to the test checker"),
                    };
                    if !fits {
                        out.push(format!("{at_or_root}: {text:?} does not match {rule}"));
                    }
                }
            }
            "format" => {
                assert_eq!(rule, "date-time", "format {rule} is not known to the test checker");
                if doc.as_str().is_some_and(|text| rfc3339::parse(text).is_none()) {
                    out.push(format!("{at_or_root}: is not a date-time"));
                }
            }
            "oneOf" => {
                let matching = rule
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|sub| is_valid(root, sub, doc))
                    .count();
                if matching != 1 {
                    out.push(format!(
                        "{at_or_root}: fits {matching} of the allowed shapes, not exactly 1"
                    ));
                }
            }
            other => panic!("schema keyword {other:?} is not known to the test checker"),
        }
    }
}

/// The fenced `json` blocks of SPEC.md.
fn spec_examples() -> Vec<Value> {
    let spec = read("SPEC.md");
    let mut examples = Vec::new();
    let mut block: Option<String> = None;
    for line in spec.lines() {
        match &mut block {
            None if line.trim() == "```json" => block = Some(String::new()),
            None => {}
            Some(text) if line.trim() == "```" => {
                examples.push(
                    serde_json::from_str(text).unwrap_or_else(|e| panic!("SPEC.md example is not JSON: {e}\n{text}")),
                );
                block = None;
            }
            Some(text) => {
                text.push_str(line);
                text.push('\n');
            }
        }
    }
    assert!(block.is_none(), "SPEC.md ends inside a json block");
    examples
}

/// The catalogue example and the data example of SPEC.md.
fn spec_pair() -> (Value, Value) {
    let examples = spec_examples();
    let [catalogue, data] = examples.as_slice() else {
        panic!(
            "expected SPEC.md to have exactly 2 json examples, found {}",
            examples.len()
        );
    };
    assert!(
        catalogue.get("contract").is_some(),
        "the first SPEC.md example should be the catalogue"
    );
    assert!(
        data.get("values").is_some(),
        "the second SPEC.md example should be the data"
    );
    (catalogue.clone(), data.clone())
}

/// Every combination of a source's choice params, as query maps.
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

const NOTHING: Vec<String> = Vec::new();

/// A mistake to make in a good document.
type Change = Box<dyn Fn(&mut Value)>;

#[test]
fn the_demo_catalogue_passes_the_schema() {
    let doc = serde_json::to_value(demo::catalogue()).unwrap();
    assert_eq!(violations(&schema("schema/catalogue.schema.json"), &doc), NOTHING);
}

#[test]
fn the_demo_data_passes_the_schema() {
    let schema = schema("schema/data.schema.json");
    let start = 1_795_478_400; // 2026-11-24T00:00:00Z
    for source in &demo::catalogue().sources {
        for choice in every_choice(source) {
            for t in (start..start + 3600).step_by(13) {
                let doc = serde_json::to_value(demo::data(&source.id, &choice, t).unwrap()).unwrap();
                assert_eq!(violations(&schema, &doc), NOTHING, "{} {choice:?} at {t}", source.id);
            }
        }
    }
}

#[test]
fn the_spec_examples_pass_the_schemas() {
    let (catalogue, data) = spec_pair();
    assert_eq!(violations(&schema("schema/catalogue.schema.json"), &catalogue), NOTHING);
    assert_eq!(violations(&schema("schema/data.schema.json"), &data), NOTHING);
}

#[test]
fn the_spec_examples_follow_the_rules_a_schema_cannot_check() {
    let (catalogue, data) = spec_pair();
    let catalogue: Catalogue = serde_json::from_value(catalogue).unwrap();
    let data: Data = serde_json::from_value(data).unwrap();
    assert_eq!(catalogue.problems(), NOTHING);
    assert_eq!(catalogue.sources[0].data_problems(&data), NOTHING);
}

#[test]
fn the_spec_says_the_right_status_for_its_own_example() {
    // "In the example, cpu at 41.5 is ok, and disk_free at 18.4 is crit."
    let (catalogue, data) = spec_pair();
    let catalogue: Catalogue = serde_json::from_value(catalogue).unwrap();
    let data: Data = serde_json::from_value(data).unwrap();
    let status = |id: &str| catalogue.sources[0].field(id).unwrap().status_of(&data.values[id]);
    assert_eq!(status("cpu"), Some(Status::Ok));
    assert_eq!(status("cpu_history"), Some(Status::Ok));
    assert_eq!(status("disk_free"), Some(Status::Crit));
    assert_eq!(status("backup"), Some(Status::Warn));
    assert_eq!(status("uptime"), None);
}

#[test]
fn the_spec_example_is_a_cut_down_demo_catalogue() {
    // The example in SPEC.md should not drift from the real demo app.
    let (example, _) = spec_pair();
    let example: Catalogue = serde_json::from_value(example).unwrap();
    let demo = demo::catalogue();
    assert_eq!(
        (example.app.as_str(), example.title.as_str()),
        (demo.app.as_str(), demo.title.as_str())
    );
    for source in &example.sources {
        let real = demo
            .source(&source.id)
            .unwrap_or_else(|| panic!("the demo has no source {:?}", source.id));
        assert_eq!((&source.title, source.refresh_s), (&real.title, real.refresh_s));
        for field in &source.fields {
            assert_eq!(Some(field), real.field(&field.id), "field {:?}", field.id);
        }
    }
}

#[test]
fn the_catalogue_schema_refuses_mistakes() {
    let schema = schema("schema/catalogue.schema.json");
    let (good, _) = spec_pair();
    let cases: Vec<(&str, Change)> = vec![
        ("a v1 catalogue", Box::new(|c| c["contract"] = json!(1))),
        ("contract as text", Box::new(|c| c["contract"] = json!("2"))),
        (
            "no contract",
            Box::new(|c| drop(c.as_object_mut().unwrap().remove("contract"))),
        ),
        (
            "an app name that is not an id",
            Box::new(|c| c["app"] = json!("My App")),
        ),
        ("an empty title", Box::new(|c| c["title"] = json!(""))),
        ("no sources", Box::new(|c| c["sources"] = json!([]))),
        (
            "a property that is not in the spec",
            Box::new(|c| c["version"] = json!("1.0")),
        ),
        (
            "a misspelt refresh_s",
            Box::new(|c| {
                let source = c["sources"][0].as_object_mut().unwrap();
                let refresh = source.remove("refresh_s").unwrap();
                source.insert("refresh".into(), refresh);
            }),
        ),
        ("refresh_s of 0", Box::new(|c| c["sources"][0]["refresh_s"] = json!(0))),
        (
            "refresh_s with a fraction",
            Box::new(|c| c["sources"][0]["refresh_s"] = json!(2.5)),
        ),
        (
            "a source with no fields",
            Box::new(|c| c["sources"][0]["fields"] = json!([])),
        ),
        (
            "a field type that does not exist",
            Box::new(|c| c["sources"][0]["fields"][0]["type"] = json!("gauge")),
        ),
        (
            "a misspelt unit",
            Box::new(|c| c["sources"][0]["fields"][0]["uint"] = json!("%")),
        ),
        (
            "thresholds on a text field",
            Box::new(|c| c["sources"][0]["fields"][3]["warn"] = json!(5)),
        ),
        (
            "a unit on a status field",
            Box::new(|c| c["sources"][0]["fields"][4]["unit"] = json!("%")),
        ),
        (
            "thresholds on a list field",
            Box::new(|c| c["sources"][0]["fields"][5]["crit"] = json!(5)),
        ),
        (
            "a threshold as text",
            Box::new(|c| c["sources"][0]["fields"][0]["warn"] = json!("75")),
        ),
        (
            "a choice with no options",
            Box::new(|c| drop(c["sources"][0]["params"][0].as_object_mut().unwrap().remove("options"))),
        ),
        (
            "a choice with an empty options list",
            Box::new(|c| c["sources"][0]["params"][0]["options"] = json!([])),
        ),
        (
            "options on a text param",
            Box::new(|c| c["sources"][0]["params"][0]["type"] = json!("text")),
        ),
        (
            "a param with no default",
            Box::new(|c| drop(c["sources"][0]["params"][0].as_object_mut().unwrap().remove("default"))),
        ),
        (
            "a default that is not text",
            Box::new(|c| c["sources"][0]["params"][0]["default"] = json!(30)),
        ),
        (
            "a view that does not exist",
            Box::new(|c| c["presets"][0]["view"] = json!("gauge")),
        ),
        (
            "a preset with no fields",
            Box::new(|c| c["presets"][0]["fields"] = json!([])),
        ),
        (
            "a preset param that is not text",
            Box::new(|c| c["presets"][0]["params"] = json!({"points": 30})),
        ),
    ];
    for (what, change) in cases {
        let mut doc = good.clone();
        change(&mut doc);
        assert_ne!(violations(&schema, &doc), NOTHING, "the schema accepted {what}");
    }
}

#[test]
fn the_data_schema_refuses_mistakes() {
    let schema = schema("schema/data.schema.json");
    let (_, good) = spec_pair();
    let cases: Vec<(&str, Change)> = vec![
        (
            "no updated_at",
            Box::new(|d| drop(d.as_object_mut().unwrap().remove("updated_at"))),
        ),
        (
            "updated_at that is not a date-time",
            Box::new(|d| d["updated_at"] = json!("just now")),
        ),
        (
            "updated_at as a number",
            Box::new(|d| d["updated_at"] = json!(1_795_512_605)),
        ),
        (
            "no values",
            Box::new(|d| drop(d.as_object_mut().unwrap().remove("values"))),
        ),
        (
            "a link to another kind of address",
            Box::new(|d| d["link"] = json!("javascript:alert(1)")),
        ),
        (
            "a link that starts with two slashes",
            Box::new(|d| d["link"] = json!("//other.example/x")),
        ),
        ("a link that is null", Box::new(|d| d["link"] = json!(null))),
        (
            "a property that is not in the spec",
            Box::new(|d| d["app"] = json!("demo")),
        ),
        (
            "a field key that is not an id",
            Box::new(|d| d["values"]["CPU load"] = json!({"value": 1})),
        ),
        ("an entry with no value", Box::new(|d| d["values"]["cpu"] = json!({}))),
        (
            "a bare value instead of an entry",
            Box::new(|d| d["values"]["cpu"] = json!(41.5)),
        ),
        (
            "a value that is an object",
            Box::new(|d| d["values"]["cpu"]["value"] = json!({"n": 1})),
        ),
        (
            "a value that is true",
            Box::new(|d| d["values"]["cpu"]["value"] = json!(true)),
        ),
        (
            "a status that does not exist",
            Box::new(|d| d["values"]["cpu"]["status"] = json!("fine")),
        ),
        (
            "a misspelt status",
            Box::new(|d| d["values"]["cpu"]["state"] = json!("ok")),
        ),
        (
            "a series of texts",
            Box::new(|d| d["values"]["cpu_history"]["series"] = json!(["1", "2"])),
        ),
        (
            "a list of bare texts",
            Box::new(|d| d["values"]["services"]["value"] = json!(["web", "mail"])),
        ),
        (
            "a list item with no label",
            Box::new(|d| d["values"]["services"]["value"][0] = json!({"value": 1})),
        ),
        (
            "a list item with a null value",
            Box::new(|d| d["values"]["services"]["value"][0]["value"] = json!(null)),
        ),
    ];
    for (what, change) in cases {
        let mut doc = good.clone();
        change(&mut doc);
        assert_ne!(violations(&schema, &doc), NOTHING, "the schema accepted {what}");
    }
}

#[test]
fn null_values_and_gaps_pass_the_data_schema() {
    let schema = schema("schema/data.schema.json");
    let (_, mut doc) = spec_pair();
    for (_, entry) in doc["values"].as_object_mut().unwrap() {
        entry["value"] = json!(null);
    }
    doc["values"]["cpu_history"]["series"] = json!([1.5, null, 2]);
    doc.as_object_mut().unwrap().remove("link");
    assert_eq!(violations(&schema, &doc), NOTHING);
}
