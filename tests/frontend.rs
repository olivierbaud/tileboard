//! Runs the frontend's own tests (`tests/web/*.test.js`) with Node's built-in test runner.
//!
//! Node is not needed to build or run tileboard, and no package is installed:
//! if Node is not on this machine, the test says so and passes.

use std::process::Command;

#[test]
fn the_frontend_tests_pass() {
    let node_is_here = Command::new("node").arg("--version").output().is_ok_and(|output| output.status.success());
    if !node_is_here {
        eprintln!("skipped: Node is not installed, so tests/web/*.test.js did not run");
        return;
    }

    let output = Command::new("node")
        .args(["--test", "tests/web/*.test.js"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("node --test should start");
    assert!(
        output.status.success(),
        "the frontend tests failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
