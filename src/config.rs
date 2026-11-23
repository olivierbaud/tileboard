//! The dashboard's private config file (`config.toml`): where it listens and which apps it shows.
//! `config.example.toml` is the documented example.

use std::collections::HashSet;
use std::fmt;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};

use axum::http::Uri;
use serde::Deserialize;

use crate::contract::is_id;

pub const DEFAULT_LISTEN: &str = "127.0.0.1:4700";
pub const DEFAULT_LAYOUT: &str = "layout.json";

#[derive(Debug, Clone)]
pub struct Config {
    pub listen: SocketAddr,
    /// Where the layout is saved. `None` keeps it in memory only.
    pub layout: Option<PathBuf>,
    pub apps: Vec<AppConfig>,
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    /// The owner's name for the app. Tiles refer to it.
    pub name: String,
    /// Where the dashboard server reaches the app. No trailing slash.
    pub base_url: String,
    /// Where a person's browser reaches the app, for links. No trailing slash.
    pub public_url: String,
    pub token: Option<Token>,
}

/// An app's bearer token. It never shows up in debug output.
#[derive(Clone, PartialEq, Eq)]
pub struct Token(String);

impl Token {
    pub fn reveal(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Token(hidden)")
    }
}

// What the file holds, before checking. No `Debug`: it carries tokens.

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    listen: Option<String>,
    layout: Option<String>,
    #[serde(default)]
    apps: Vec<RawApp>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawApp {
    name: String,
    base_url: String,
    public_url: Option<String>,
    token: Option<String>,
}

impl Config {
    /// Reads a config file. A relative `layout` path is relative to the file.
    pub fn load(path: &Path) -> Result<Config, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        let dir = path.parent().unwrap_or(Path::new(""));
        Config::parse(&text, dir).map_err(|problems| format!("{} is not valid:\n{problems}", path.display()))
    }

    /// Parses config text. `dir` is where a relative `layout` path starts from.
    pub fn parse(text: &str, dir: &Path) -> Result<Config, String> {
        let raw: RawConfig = toml::from_str(text).map_err(|e| e.to_string())?;
        let mut problems = Vec::new();

        let listen_text = raw.listen.as_deref().unwrap_or(DEFAULT_LISTEN);
        let listen = listen_text.parse::<SocketAddr>().ok();
        if listen.is_none() {
            problems.push(format!("listen: {listen_text:?} is not an ip:port address"));
        }

        let layout = raw.layout.as_deref().unwrap_or(DEFAULT_LAYOUT);
        if layout.is_empty() {
            problems.push("layout: the path is empty".to_string());
        }

        let mut names = HashSet::new();
        let mut apps = Vec::new();
        for (i, app) in raw.apps.into_iter().enumerate() {
            let at = if is_id(&app.name) { format!("app {:?}", app.name) } else { format!("app number {}", i + 1) };
            if !is_id(&app.name) {
                problems.push(format!("{at}: name must be lowercase letters, digits, _ or -, at most 64 long"));
            } else if !names.insert(app.name.clone()) {
                problems.push(format!("{at}: the name is used twice"));
            }

            let base_url = check_url(&app.base_url, false).map_err(|p| problems.push(format!("{at}: base_url {p}")));
            let public_url = match &app.public_url {
                Some(url) => check_url(url, true).map_err(|p| problems.push(format!("{at}: public_url {p}"))),
                None => base_url.clone(),
            };
            let token_ok =
                app.token.as_deref().is_none_or(|t| !t.is_empty() && t.bytes().all(|b| b.is_ascii_graphic()));
            if !token_ok {
                problems.push(format!("{at}: token must be visible characters with no spaces"));
            }

            if let (Ok(base_url), Ok(public_url)) = (base_url, public_url) {
                apps.push(AppConfig { name: app.name, base_url, public_url, token: app.token.map(Token) });
            }
        }

        match listen {
            Some(listen) if problems.is_empty() => Ok(Config { listen, layout: Some(dir.join(layout)), apps }),
            _ => Err(problems.join("\n")),
        }
    }

    /// The built-in config of `--demo`: one app, the demo app, and a layout that is not saved.
    pub fn demo(listen: SocketAddr, demo_app: SocketAddr) -> Config {
        // "Every address" is where the demo app listens, not an address to call.
        let ip = if demo_app.ip().is_unspecified() { IpAddr::V4(Ipv4Addr::LOCALHOST) } else { demo_app.ip() };
        let url = format!("http://{}", SocketAddr::new(ip, demo_app.port()));
        let demo = AppConfig { name: "demo".to_string(), base_url: url.clone(), public_url: url, token: None };
        Config { listen, layout: None, apps: vec![demo] }
    }
}

/// Checks an app address and returns it without trailing slashes.
fn check_url(text: &str, https_allowed: bool) -> Result<String, String> {
    let (scheme, rest) = if let Some(rest) = text.strip_prefix("http://") {
        ("http://", rest)
    } else if let Some(rest) = text.strip_prefix("https://") {
        if !https_allowed {
            return Err("uses https://, which this dashboard cannot call yet; use http://".to_string());
        }
        ("https://", rest)
    } else if https_allowed {
        return Err("must start with http:// or https://".to_string());
    } else {
        return Err("must start with http://".to_string());
    };
    if rest.contains(['?', '#']) {
        return Err("must not have a query or a fragment".to_string());
    }

    let url = format!("{scheme}{}", rest.trim_end_matches('/'));
    let not_valid = || "is not a valid address".to_string();
    let uri: Uri = url.parse().map_err(|_| not_valid())?;
    match uri.authority() {
        Some(authority) if authority.as_str().contains('@') => Err("must not hold a user name or password".to_string()),
        Some(authority) if !authority.host().is_empty() => Ok(url),
        _ => Err(not_valid()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<Config, String> {
        Config::parse(text, Path::new("/etc/tileboard"))
    }

    fn problem(text: &str) -> String {
        parse(text).expect_err("this config should be refused")
    }

    const ONE_APP: &str = "[[apps]]\nname = \"demo\"\nbase_url = \"http://127.0.0.1:4701\"\n";

    #[test]
    fn the_example_file_is_a_valid_config() {
        let config = parse(include_str!("../config.example.toml")).unwrap();
        assert_eq!(config.listen, "127.0.0.1:4700".parse().unwrap());
        assert_eq!(config.layout, Some(PathBuf::from("/etc/tileboard").join("layout.json")));
        assert_eq!(config.apps.len(), 1);
        let demo = &config.apps[0];
        assert_eq!((demo.name.as_str(), demo.base_url.as_str()), ("demo", "http://127.0.0.1:4701"));
        assert_eq!(demo.public_url, demo.base_url);
        assert_eq!(demo.token, None);
    }

    #[test]
    fn everything_can_be_set() {
        let config = parse(
            r#"
            listen = "0.0.0.0:8080"
            layout = "data/tiles.json"

            [[apps]]
            name = "shop"
            base_url = "http://127.0.0.1:9000/shop/"
            public_url = "https://shop.example"
            token = "s3cr3t-token"

            [[apps]]
            name = "other-app_2"
            base_url = "http://other.example:81"
            "#,
        )
        .unwrap();
        assert_eq!(config.listen, "0.0.0.0:8080".parse().unwrap());
        assert_eq!(config.layout, Some(PathBuf::from("/etc/tileboard").join("data/tiles.json")));
        let shop = &config.apps[0];
        assert_eq!(shop.base_url, "http://127.0.0.1:9000/shop");
        assert_eq!(shop.public_url, "https://shop.example");
        assert_eq!(shop.token.as_ref().unwrap().reveal(), "s3cr3t-token");
        assert_eq!(config.apps[1].public_url, "http://other.example:81");
    }

    #[test]
    fn an_empty_file_is_a_dashboard_with_no_apps() {
        let config = parse("").unwrap();
        assert_eq!(config.listen, DEFAULT_LISTEN.parse().unwrap());
        assert!(config.apps.is_empty());
    }

    #[test]
    fn a_token_never_shows_in_debug_output() {
        let config = parse(&format!("{ONE_APP}token = \"s3cr3t-token\"\n")).unwrap();
        let shown = format!("{config:?}");
        assert!(!shown.contains("s3cr3t"), "{shown}");
        assert!(shown.contains("Token(hidden)"));
    }

    #[test]
    fn mistakes_are_explained() {
        let cases = [
            ("listen = \"localhost\"", "listen: \"localhost\" is not an ip:port address"),
            ("layout = \"\"", "layout: the path is empty"),
            ("port = 4700", "unknown field `port`"),
            ("[[apps]]\nname = \"demo\"", "missing field `base_url`"),
            ("[[apps]]\nname = \"demo\"\nbase_url = \"http://a.example\"\nurl = \"x\"", "unknown field `url`"),
            ("[[apps]]\nname = \"My App\"\nbase_url = \"http://a.example\"", "app number 1: name must be"),
            (
                "[[apps]]\nname = \"demo\"\nbase_url = \"a.example:80\"",
                "app \"demo\": base_url must start with http://",
            ),
            ("[[apps]]\nname = \"demo\"\nbase_url = \"ftp://a.example\"", "base_url must start with http://"),
            ("[[apps]]\nname = \"demo\"\nbase_url = \"https://a.example\"", "cannot call yet"),
            (
                "[[apps]]\nname = \"demo\"\nbase_url = \"http://me:pw@a.example\"",
                "must not hold a user name or password",
            ),
            ("[[apps]]\nname = \"demo\"\nbase_url = \"http://a.example/?x=1\"", "must not have a query or a fragment"),
            ("[[apps]]\nname = \"demo\"\nbase_url = \"http://a b\"", "base_url is not a valid address"),
            ("[[apps]]\nname = \"demo\"\nbase_url = \"http://\"", "base_url is not a valid address"),
            (
                "[[apps]]\nname = \"demo\"\nbase_url = \"http://a.example\"\npublic_url = \"mailto:me\"",
                "public_url must start with http:// or https://",
            ),
            (
                "[[apps]]\nname = \"demo\"\nbase_url = \"http://a.example\"\ntoken = \"\"",
                "token must be visible characters",
            ),
            (
                "[[apps]]\nname = \"demo\"\nbase_url = \"http://a.example\"\ntoken = \"a b\"",
                "token must be visible characters",
            ),
        ];
        for (text, expected) in cases {
            let found = problem(text);
            assert!(found.contains(expected), "for {text:?}: expected {expected:?} in {found:?}");
        }
    }

    #[test]
    fn a_name_cannot_be_used_twice() {
        assert!(problem(&format!("{ONE_APP}{ONE_APP}")).contains("app \"demo\": the name is used twice"));
    }

    #[test]
    fn every_problem_is_reported_at_once() {
        let found = problem("listen = \"nope\"\n[[apps]]\nname = \"A\"\nbase_url = \"x\"\n");
        assert_eq!(found.lines().count(), 3, "{found}");
    }

    #[test]
    fn the_demo_config_calls_the_demo_app_on_loopback() {
        let listen = "127.0.0.1:4700".parse().unwrap();
        let config = Config::demo(listen, "0.0.0.0:4701".parse().unwrap());
        assert_eq!(config.apps[0].base_url, "http://127.0.0.1:4701");
        assert_eq!(config.layout, None);
        let config = Config::demo(listen, "127.0.0.1:9999".parse().unwrap());
        assert_eq!(config.apps[0].base_url, "http://127.0.0.1:9999");
    }
}
