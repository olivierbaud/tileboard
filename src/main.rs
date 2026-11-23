//! `tileboard --demo` runs the demo widget app (made-up data).
//! The dashboard itself is the next step of `docs/plan.md`.

use std::net::SocketAddr;
use std::process::ExitCode;

const DEMO_ADDR: &str = "127.0.0.1:4701";

const USAGE: &str = "\
tileboard: a modular dashboard

Usage:
  tileboard --demo [--demo-addr <ip:port>]

Options:
  --demo                  Run the demo widget app (made-up data)
  --demo-addr <ip:port>   Where the demo app listens [default: 127.0.0.1:4701]
  -h, --help              Show this help
  -V, --version           Show the version";

#[derive(Debug, PartialEq)]
enum Command {
    Help,
    Version,
    Demo { addr: SocketAddr },
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Command, String> {
    let mut demo = false;
    let mut addr: SocketAddr = DEMO_ADDR.parse().expect("the default address is valid");

    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "-V" | "--version" => return Ok(Command::Version),
            "--demo" => demo = true,
            "--demo-addr" => {
                let value = args
                    .next()
                    .ok_or("--demo-addr needs a value, for example 127.0.0.1:4701")?;
                addr = value
                    .parse()
                    .map_err(|_| format!("--demo-addr: {value:?} is not an ip:port address"))?;
            }
            other => return Err(format!("unknown argument {other:?}")),
        }
    }

    if demo {
        Ok(Command::Demo { addr })
    } else {
        Err("the dashboard is not built yet; run with --demo to start the demo widget app".to_string())
    }
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
        Ok(Command::Demo { addr }) => run_demo(addr).await,
        Err(message) => {
            eprintln!("tileboard: {message}\n\n{USAGE}");
            ExitCode::from(2)
        }
    }
}

async fn run_demo(addr: SocketAddr) -> ExitCode {
    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(listener) => listener,
        Err(e) => {
            eprintln!("tileboard: cannot listen on {addr}: {e}");
            return ExitCode::FAILURE;
        }
    };
    let addr = listener.local_addr().unwrap_or(addr);
    println!("Demo widget app (made-up data): http://{addr}/widgets");
    println!("Stop with Ctrl+C.");

    match tileboard::demo::serve(listener).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("tileboard: the demo app stopped: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Command, String> {
        parse_args(args.iter().map(|arg| arg.to_string()))
    }

    #[test]
    fn demo_listens_on_loopback_by_default() {
        assert_eq!(
            parse(&["--demo"]),
            Ok(Command::Demo {
                addr: "127.0.0.1:4701".parse().unwrap()
            })
        );
    }

    #[test]
    fn demo_addr_can_be_set_before_or_after() {
        let expected = Ok(Command::Demo {
            addr: "127.0.0.1:9000".parse().unwrap(),
        });
        assert_eq!(parse(&["--demo", "--demo-addr", "127.0.0.1:9000"]), expected);
        assert_eq!(parse(&["--demo-addr", "127.0.0.1:9000", "--demo"]), expected);
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
        assert!(parse(&[]).unwrap_err().contains("--demo"));
        assert!(
            parse(&["--demo", "--port", "1"])
                .unwrap_err()
                .contains("unknown argument \"--port\"")
        );
        assert!(parse(&["--demo", "--demo-addr"]).unwrap_err().contains("needs a value"));
        assert!(
            parse(&["--demo", "--demo-addr", "localhost"])
                .unwrap_err()
                .contains("not an ip:port")
        );
    }
}
