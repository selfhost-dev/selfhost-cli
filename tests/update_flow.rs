//! End-to-end tests for `selfhost update --check`.
//!
//! The check runs against a release index served on `127.0.0.1` by the test,
//! so the network path, the parsing and the reporting are all exercised
//! without reaching cli.selfhost.dev. `--check` never writes, which is what
//! makes it safe to point at the binary cargo just built: the suite proves
//! that, rather than assuming it, by comparing the executable's timestamp
//! before and after the run.
use std::io::{Read as _, Write as _};

use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::SystemTime;

use assert_cmd::Command;
use predicates::prelude::*;

/// The CLI under test, pointed at a private home so the store can never be the
/// developer's own.
fn selfhost() -> Command {
    let mut command = Command::cargo_bin("selfhost").expect("the selfhost binary is built");
    command.env("HOME", std::env::temp_dir().join("selfhost-update-flow"));
    for name in ["SELFHOSTDEV_PROFILE", "SELFHOST_MANIFEST_URL"] {
        command.env_remove(name);
    }
    command
}

/// A release index on `127.0.0.1`, answering every request with the same
/// document, for as long as the test holds it.
struct Index {
    port: u16,
    stop: Arc<AtomicBool>,
}

impl Index {
    fn serve(document: &str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let port = listener.local_addr().expect("the bound address").port();
        let stop = Arc::new(AtomicBool::new(false));
        let finished = Arc::clone(&stop);
        let document = document.to_string();
        thread::spawn(move || {
            for stream in listener.incoming() {
                if finished.load(Ordering::SeqCst) {
                    return;
                }
                let Ok(mut stream) = stream else { continue };
                let mut request = [0u8; 2048];
                let _ = stream.read(&mut request);
                let answer = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{document}",
                    document.len()
                );
                let _ = stream.write_all(answer.as_bytes());
                let _ = stream.flush();
            }
        });
        Self { port, stop }
    }

    fn url(&self) -> String {
        format!("http://127.0.0.1:{}/latest.json", self.port)
    }
}

impl Drop for Index {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // Wake the blocked `accept` so the thread sees the flag and returns.
        let _ = TcpStream::connect(("127.0.0.1", self.port));
    }
}

/// The key the running platform publishes under.
fn platform() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

/// The version of the binary under test. Read from the crate rather than
/// written down, so a release bump does not turn these tests red.
fn local_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// A version strictly newer than the one under test, so the check has something
/// to report. Bumping the major is enough and stays newer through any minor or
/// patch release of the same build.
fn newer_version() -> String {
    let current = semver::Version::parse(env!("CARGO_PKG_VERSION")).expect("a valid crate version");
    format!("{}.0.0", current.major + 1)
}

/// A release index for `version`, with a download this test never fetches.
fn index(version: &str) -> String {
    format!(
        "{{\"version\": {version:?}, \"notes\": \"selfhost {version}.\", \"assets\": {{\"{key}\": \"https://cli.selfhost.dev/v{version}/selfhost-{key}\"}}, \"sha256\": {{\"{key}\": \"{}\"}}}}",
        "0".repeat(64),
        key = platform(),
    )
}

/// When the executable under test was last written, for the "writes nothing"
/// comparison.
fn built_at() -> SystemTime {
    std::fs::metadata(Path::new(env!("CARGO_BIN_EXE_selfhost")))
        .expect("the binary is on disk")
        .modified()
        .expect("the binary has a timestamp")
}

/// The check reports both versions and whether an update is waiting, and
/// leaves the executable exactly as it was.
#[test]
fn check_reports_the_installed_and_the_newest_version() {
    let newer = newer_version();
    let index = Index::serve(&index(&newer));
    let before = built_at();

    selfhost()
        .args([
            "update",
            "--check",
            "--manifest-url",
            &index.url(),
            "-o",
            "table",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(local_version()))
        .stdout(predicate::str::contains(newer.clone()))
        .stdout(predicate::str::contains("update available: yes"));

    assert_eq!(built_at(), before, "a check wrote to the executable");
}

/// A build that is already current says so, and still names both versions.
#[test]
fn check_says_so_when_there_is_nothing_to_install() {
    let index = Index::serve(&index(&local_version()));

    selfhost()
        .args([
            "update",
            "--check",
            "--manifest-url",
            &index.url(),
            "-o",
            "table",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("update available: no"));
}

/// Piped output is JSON, in the shape scripts read.
#[test]
fn check_reports_json_when_it_is_piped() {
    let newer = newer_version();
    let index = Index::serve(&index(&newer));

    let assert = selfhost()
        .args(["update", "--check", "--manifest-url", &index.url()])
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout);
    let report: serde_json::Value =
        serde_json::from_str(&stdout).unwrap_or_else(|err| panic!("{stdout}: {err}"));

    assert_eq!(report["current"], local_version());
    assert_eq!(report["latest"], newer);
    assert_eq!(report["update_available"], true);
    assert_eq!(report["updated"], false);
}

/// The address of the index is checked before anything is sent: plain http
/// off this machine is refused.
#[test]
fn a_plain_http_index_is_refused() {
    selfhost()
        .args([
            "update",
            "--check",
            "--manifest-url",
            "http://cli.selfhost.dev/latest.json",
        ])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("must use https"));
}

/// The command is registered, with a one-line about of its own.
#[test]
fn update_is_registered_with_its_flags() {
    let assert = selfhost().args(["update", "--help"]).assert().success();
    let help = String::from_utf8_lossy(&assert.get_output().stdout);

    assert!(help.contains("--check"), "{help}");
    assert!(help.contains("--manifest-url"), "{help}");
    assert!(!help.contains('`'), "{help}");

    selfhost()
        .arg("tree")
        .assert()
        .success()
        .stdout(predicate::str::contains("selfhost update"));
}
