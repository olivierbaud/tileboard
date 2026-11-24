//! The `tileboard` program: the dashboard server, and the demo widget app on request.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use tileboard::config::{Config, DEFAULT_LISTEN};
use tileboard::dashboard::{self, Dashboard};
use tileboard::demo;
use tileboard::hub::Hub;
use tileboard::layout::{Layout, LayoutStore};
use tileboard::web::Web;

const DEMO_ADDR: &str = "127.0.0.1:4701";

const USAGE: &str = "\
tileboard: a modular dashboard

Usage:
  tileboard --demo                  Try it: the dashboard plus a demo app, no config needed
  tileboard [--config <file>]       Run the dashboard with your config

Options:
  --config <file>         The config file [default: config.toml]
  --listen <ip:port>      Where the dashboard listens (wins over the config file)
  --demo                  Dashboard + demo app with made-up data; no config, nothing is saved
  --demo-app              Also run the demo app next to your own config
  --demo-addr <ip:port>   Where the demo app listens [default: 127.0.0.1:4701]
  --web-dir <folder>      Serve the frontend files from a folder instead of the built-in ones
  -h, --help              Show this help
  -V, --version           Show the version";

#[derive(Debug, PartialEq)]
enum Command {
    Help,
    Version,
    Run(Options),
}

#[derive(Debug, PartialEq)]
struct Options {
    /// The built-in config and starter layout, with the demo app.
    demo: bool,
    /// Run the demo app in this process.
    demo_app: bool,
    demo_addr: SocketAddr,
    config: PathBuf,
    listen: Option<SocketAddr>,
    web_dir: Option<PathBuf>,
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Command, String> {
    let mut options = Options {
        demo: false,
        demo_app: false,
        demo_addr: DEMO_ADDR.parse().expect("the default address is valid"),
        config: PathBuf::from("config.toml"),
        listen: None,
        web_dir: None,
    };
    let mut config_given = false;

    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        let mut value = |example: &str| args.next().ok_or(format!("{arg} needs a value, for example {example}"));
        let address = |text: String| text.parse().map_err(|_| format!("{arg}: {text:?} is not an ip:port address"));
        match arg.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "-V" | "--version" => return Ok(Command::Version),
            "--demo" => options.demo = true,
            "--demo-app" => options.demo_app = true,
            "--demo-addr" => options.demo_addr = address(value(DEMO_ADDR)?)?,
            "--listen" => options.listen = Some(address(value(DEFAULT_LISTEN)?)?),
            "--config" => {
                options.config = PathBuf::from(value("config.toml")?);
                config_given = true;
            }
            "--web-dir" => options.web_dir = Some(PathBuf::from(value("web")?)),
            other => return Err(format!("unknown argument {other:?}")),
        }
    }

    if options.demo && config_given {
        return Err("--demo brings its own config; leave out --config (or use --demo-app with it)".to_string());
    }
    options.demo_app |= options.demo;
    Ok(Command::Run(options))
}

#[tokio::main]
async fn main() -> ExitCode {
    match parse_args(std::env::args().skip(1)) {
        Ok(Command::Help) => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        Ok(Command::Version) => {
            println!("tileboard {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Ok(Command::Run(options)) => match run(options).await {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                eprintln!("tileboard: {message}");
                ExitCode::FAILURE
            }
        },
        Err(message) => {
            eprintln!("tileboard: {message}\n\n{USAGE}");
            ExitCode::from(2)
        }
    }
}

async fn run(options: Options) -> Result<(), String> {
    let mut config = if options.demo {
        Config::demo(DEFAULT_LISTEN.parse().expect("the default address is valid"), options.demo_addr)
    } else {
        Config::load(&options.config).map_err(|problem| {
            format!("{problem}\n\nCopy config.example.toml to config.toml and edit it, or try `tileboard --demo`.")
        })?
    };
    if let Some(listen) = options.listen {
        config.listen = listen;
    }

    let starter = if options.demo { demo::starter_layout(&config.apps[0].name) } else { Layout::default() };
    let layout = LayoutStore::open(config.layout.clone(), starter)?;
    let web = options.web_dir.map_or_else(Web::built_in, Web::from_dir);

    let listen = |addr: SocketAddr| async move {
        tokio::net::TcpListener::bind(addr).await.map_err(|e| format!("cannot listen on {addr}: {e}"))
    };
    let dashboard_listener = listen(config.listen).await?;
    let demo_listener = if options.demo_app { Some(listen(options.demo_addr).await?) } else { None };

    println!("Dashboard: http://{}", dashboard_listener.local_addr().unwrap_or(config.listen));
    if let Some(listener) = &demo_listener {
        println!("Demo app (made-up data): http://{}/widgets", listener.local_addr().unwrap_or(options.demo_addr));
    }
    for app in &config.apps {
        println!("App {}: {}", app.name, app.base_url);
    }
    match &config.layout {
        Some(path) => println!("Layout: {}", path.display()),
        None => println!("Layout: kept in memory, not saved"),
    }
    if !config.listen.ip().is_loopback() {
        println!("The dashboard has no login. Keep {} on a private network.", config.listen);
    }
    println!("Stop with Ctrl+C.");

    let mut dashboard = Dashboard::new(Hub::new(config.apps), layout, web);
    if config.manage_apps {
        println!("Apps can be added and removed from the settings page.");
        dashboard = dashboard.managing_apps(config.apps_file);
    }
    let dashboard = Arc::new(dashboard);
    let dashboard = dashboard::serve(dashboard_listener, dashboard);
    let stopped = match demo_listener {
        Some(listener) => tokio::select! {
            stopped = dashboard => stopped,
            stopped = demo::serve(listener) => stopped,
        },
        None => dashboard.await,
    };
    stopped.map_err(|e| format!("the server stopped: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Command, String> {
        parse_args(args.iter().map(|arg| arg.to_string()))
    }

    fn options(args: &[&str]) -> Options {
        match parse(args) {
            Ok(Command::Run(options)) => options,
            other => panic!("expected a run, got {other:?}"),
        }
    }

    #[test]
    fn with_no_arguments_the_dashboard_runs_from_config_toml() {
        let options = options(&[]);
        assert_eq!(options.config, PathBuf::from("config.toml"));
        assert!(!options.demo && !options.demo_app);
        assert_eq!((options.listen, options.web_dir), (None, None));
    }

    #[test]
    fn demo_brings_the_demo_app_on_loopback() {
        let options = options(&["--demo"]);
        assert!(options.demo && options.demo_app);
        assert_eq!(options.demo_addr, "127.0.0.1:4701".parse().unwrap());
    }

    #[test]
    fn the_demo_app_can_run_next_to_a_real_config() {
        let options =
            options(&["--config", "/etc/tileboard/config.toml", "--demo-app", "--demo-addr", "127.0.0.1:9000"]);
        assert!(options.demo_app && !options.demo);
        assert_eq!(options.config, PathBuf::from("/etc/tileboard/config.toml"));
        assert_eq!(options.demo_addr, "127.0.0.1:9000".parse().unwrap());
    }

    #[test]
    fn listen_and_web_dir_can_be_set() {
        let options = options(&["--demo", "--listen", "0.0.0.0:8080", "--web-dir", "web"]);
        assert_eq!(options.listen, Some("0.0.0.0:8080".parse().unwrap()));
        assert_eq!(options.web_dir, Some(PathBuf::from("web")));
    }

    #[test]
    fn help_and_version() {
        assert_eq!(parse(&["--help"]), Ok(Command::Help));
        assert_eq!(parse(&["-h"]), Ok(Command::Help));
        assert_eq!(parse(&["--version"]), Ok(Command::Version));
        assert_eq!(parse(&["-V"]), Ok(Command::Version));
    }

    #[test]
    fn mistakes_are_explained() {
        assert!(parse(&["--port", "1"]).unwrap_err().contains("unknown argument \"--port\""));
        assert!(parse(&["--demo", "--demo-addr"]).unwrap_err().contains("--demo-addr needs a value"));
        assert!(parse(&["--listen", "localhost"]).unwrap_err().contains("--listen: \"localhost\" is not an ip:port"));
        assert!(parse(&["--config"]).unwrap_err().contains("--config needs a value"));
        assert!(parse(&["--demo", "--config", "x.toml"]).unwrap_err().contains("--demo brings its own config"));
    }
}
