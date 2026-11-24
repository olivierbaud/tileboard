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

/// The file that keeps the apps added from the settings page, next to the layout file.
pub const APPS_FILE: &str = "apps.json";

#[derive(Debug, Clone)]
pub struct Config {
    pub listen: SocketAddr,
    /// Where the layout is saved. `None` keeps it in memory only.
    pub layout: Option<PathBuf>,
    /// Whether the settings page (and so anyone who can reach the dashboard) may check, add and remove apps.
    pub manage_apps: bool,
    /// Where apps added from the settings page are kept. `None` keeps them in memory only.
    pub apps_file: Option<PathBuf>,
    /// The apps of the config file, then the ones added from the settings page.
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
    /// Added from the settings page (kept in the apps file) rather than written in the config file.
    pub managed: bool,
}

/// An app as it is written down: in the config file, in the apps file, and in a request to add one.
/// No `Debug`: it carries a token.
#[derive(Clone, Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct AppEntry {
    pub name: String,
    pub base_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

impl AppConfig {
    /// Checks an app as written down. `Err` holds everything wrong with it, in words.
    pub fn new(entry: AppEntry, managed: bool) -> Result<AppConfig, Vec<String>> {
        let mut problems = Vec::new();
        if !is_id(&entry.name) {
            problems.push("name must be lowercase letters, digits, _ or -, at most 64 long".to_string());
        }
        let base_url = check_url(&entry.base_url, false).map_err(|p| problems.push(format!("base_url {p}")));
        let public_url = match &entry.public_url {
            Some(url) => check_url(url, true).map_err(|p| problems.push(format!("public_url {p}"))),
            None => base_url.clone(),
        };
        let token_ok = entry.token.as_deref().is_none_or(|t| !t.is_empty() && t.bytes().all(|b| b.is_ascii_graphic()));
        if !token_ok {
            problems.push("token must be visible characters with no spaces".to_string());
        }
        match (base_url, public_url) {
            (Ok(base_url), Ok(public_url)) if problems.is_empty() => {
                Ok(AppConfig { name: entry.name, base_url, public_url, token: entry.token.map(Token), managed })
            }
            _ => Err(problems),
        }
    }

    /// The app as it is written down again (for the apps file).
    pub fn entry(&self) -> AppEntry {
        AppEntry {
            name: self.name.clone(),
            base_url: self.base_url.clone(),
            public_url: (self.public_url != self.base_url).then(|| self.public_url.clone()),
            token: self.token.as_ref().map(|token| token.reveal().to_string()),
        }
    }
}

/// Reads the apps added from the settings page. A missing file means none.
pub fn load_managed_apps(path: &Path) -> Result<Vec<AppConfig>, String> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let entries: Vec<AppEntry> =
        serde_json::from_str(&text).map_err(|e| format!("{} is not an apps file: {e}", path.display()))?;
    entries
        .into_iter()
        .map(|entry| {
            let name = entry.name.clone();
            AppConfig::new(entry, true).map_err(|p| format!("{}: app {name:?}: {}", path.display(), p.join("; ")))
        })
        .collect()
}

/// Writes the apps added from the settings page: a new file swapped in, readable by its owner only (it holds tokens).
pub fn save_managed_apps(path: &Path, apps: &[AppConfig]) -> std::io::Result<()> {
    use std::io::Write;
    if let Some(dir) = path.parent().filter(|dir| !dir.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    let entries: Vec<AppEntry> = apps.iter().filter(|app| app.managed).map(AppConfig::entry).collect();
    let mut text = serde_json::to_string_pretty(&entries).map_err(std::io::Error::other)?;
    text.push('\n');

    let mut temp = path.as_os_str().to_owned();
    temp.push(".tmp");
    let temp = PathBuf::from(temp);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options.open(&temp)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&temp, path)
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
    manage_apps: Option<bool>,
    #[serde(default)]
    apps: Vec<AppEntry>,
}

impl Config {
    /// Reads a config file, and the apps added from the settings page that are kept next to the layout.
    /// A relative `layout` path is relative to the file.
    pub fn load(path: &Path) -> Result<Config, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        let dir = path.parent().unwrap_or(Path::new(""));
        let mut config =
            Config::parse(&text, dir).map_err(|problems| format!("{} is not valid:\n{problems}", path.display()))?;
        if let Some(apps_file) = &config.apps_file {
            for app in load_managed_apps(apps_file)? {
                // The config file wins: an app of the same name written there hides the one added from the page.
                if !config.apps.iter().any(|known| known.name == app.name) {
                    config.apps.push(app);
                }
            }
        }
        Ok(config)
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
        for (i, entry) in raw.apps.into_iter().enumerate() {
            let at = if is_id(&entry.name) { format!("app {:?}", entry.name) } else { format!("app number {}", i + 1) };
            if is_id(&entry.name) && !names.insert(entry.name.clone()) {
                problems.push(format!("{at}: the name is used twice"));
            }
            match AppConfig::new(entry, false) {
                Ok(app) => apps.push(app),
                Err(found) => problems.extend(found.into_iter().map(|problem| format!("{at}: {problem}"))),
            }
        }

        match listen {
            Some(listen) if problems.is_empty() => {
                let layout = dir.join(layout);
                let apps_file = layout.with_file_name(APPS_FILE);
                Ok(Config {
                    listen,
                    layout: Some(layout),
                    manage_apps: raw.manage_apps.unwrap_or(false),
                    apps_file: Some(apps_file),
                    apps,
                })
            }
            _ => Err(problems.join("\n")),
        }
    }

    /// The built-in config of `--demo`: one app, the demo app, and a layout that is not saved.
    pub fn demo(listen: SocketAddr, demo_app: SocketAddr) -> Config {
        // "Every address" is where the demo app listens, not an address to call.
        let ip = if demo_app.ip().is_unspecified() { IpAddr::V4(Ipv4Addr::LOCALHOST) } else { demo_app.ip() };
        let url = format!("http://{}", SocketAddr::new(ip, demo_app.port()));
        let demo =
            AppConfig { name: "demo".to_string(), base_url: url.clone(), public_url: url, token: None, managed: false };
        // The demo lets the settings page add apps, to try it; nothing is written to disk.
        Config { listen, layout: None, manage_apps: true, apps_file: None, apps: vec![demo] }
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
    fn changing_apps_from_the_page_is_off_unless_asked_for() {
        let config = parse(ONE_APP).unwrap();
        assert!(!config.manage_apps);
        assert_eq!(config.apps_file, Some(PathBuf::from("/etc/tileboard").join(APPS_FILE)));
        assert!(!config.apps[0].managed);
        assert!(parse(&format!("manage_apps = true\n{ONE_APP}")).unwrap().manage_apps);
        // The apps file sits next to the layout, wherever that is.
        let elsewhere = parse("layout = \"data/tiles.json\"").unwrap();
        assert_eq!(elsewhere.apps_file, Some(PathBuf::from("/etc/tileboard").join("data").join(APPS_FILE)));
        // The demo allows it, and keeps nothing on disk.
        let demo = Config::demo("127.0.0.1:4700".parse().unwrap(), "127.0.0.1:4701".parse().unwrap());
        assert!(demo.manage_apps);
        assert_eq!(demo.apps_file, None);
    }

    #[test]
    fn apps_added_from_the_page_are_kept_and_read_back() {
        let dir = std::env::temp_dir().join(format!("tileboard-test-{}-apps", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("data").join(APPS_FILE);
        let app = |name: &str, address: &str, token: Option<&str>, managed: bool| {
            let entry = AppEntry {
                name: name.to_string(),
                base_url: address.to_string(),
                public_url: None,
                token: token.map(str::to_string),
            };
            AppConfig::new(entry, managed).unwrap()
        };

        // Only the apps added from the page go into the file; the folder is made if needed.
        let all = [
            app("written", "http://w.example", None, false),
            app("one", "http://app.example:81/", None, true),
            app("two", "http://two.example", Some("s3cr3t-token"), true),
        ];
        save_managed_apps(&path, &all).unwrap();
        let back = load_managed_apps(&path).unwrap();
        assert_eq!(back.iter().map(|app| app.name.as_str()).collect::<Vec<_>>(), ["one", "two"]);
        assert!(back.iter().all(|app| app.managed));
        assert_eq!(back[0].base_url, "http://app.example:81");
        assert_eq!(back[1].token.as_ref().unwrap().reveal(), "s3cr3t-token");
        assert!(!path.with_extension("json.tmp").exists(), "the temporary file is gone");

        // Loading the config brings them in. An app of the same name in config.toml wins.
        let config_path = dir.join("config.toml");
        let text = "manage_apps = true\nlayout = \"data/layout.json\"\n[[apps]]\nname = \"one\"\nbase_url = \"http://config.example\"\n";
        std::fs::write(&config_path, text).unwrap();
        let config = Config::load(&config_path).unwrap();
        let seen: Vec<_> =
            config.apps.iter().map(|app| (app.name.as_str(), app.base_url.as_str(), app.managed)).collect();
        assert_eq!(seen, [("one", "http://config.example", false), ("two", "http://two.example", true)]);

        // No file means no apps; a broken file is an error, not silence.
        assert!(load_managed_apps(&dir.join("nope.json")).unwrap().is_empty());
        std::fs::write(&path, "{ not json").unwrap();
        assert!(load_managed_apps(&path).unwrap_err().contains("is not an apps file"));
        assert!(Config::load(&config_path).unwrap_err().contains("is not an apps file"));
        let _ = std::fs::remove_dir_all(&dir);
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
