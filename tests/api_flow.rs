//! End-to-end tests for the raw `api` escape hatch.
//!
//! Every test runs against a private `HOME`, so the store lands in a temp
//! directory and no test touches the developer's real `~/.selfhost`. Nothing
//! here needs credentials or a network: every path stops at validation, the
//! dry-run gate, the organization hint or the credential check — before any
//! request is sent.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use assert_cmd::Command;
use predicates::prelude::*;

/// The usage error a run with `{org}` in the path raises when nothing selected one.
const NO_ORGANIZATION: &str = "no organization selected; run selfhost org use <slug>";

/// A throwaway home directory, `0700`, removed on drop (best effort).
struct TempHome {
    path: PathBuf,
}

impl TempHome {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("selfhost-api-{}-{unique}", std::process::id()));
        std::fs::create_dir_all(&path).expect("the temp home is created");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))
                .expect("the temp home is private");
        }
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempHome {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// The binary under test, pointed at `home` with the ambient auth knobs cleared
/// so the test controls the whole input.
fn selfhost(home: &TempHome) -> Command {
    let mut command = Command::cargo_bin("selfhost").expect("the selfhost binary is built");
    command.env("HOME", home.path());
    for name in [
        "SELFHOSTDEV_PROFILE",
        "SELFHOSTDEV_BASE_URL",
        "SELFHOSTDEV_ORG",
        "FIREBASE_API_KEY",
        "FIREBASE_REFRESH_TOKEN",
    ] {
        command.env_remove(name);
    }
    command
}

/// Anything outside GET/POST/PUT/PATCH/DELETE is a usage error, decided before
/// any credential is read or request is sent.
#[test]
fn unknown_methods_are_usage_errors() {
    let home = TempHome::new();

    for method in ["FROB", "getall", ""] {
        selfhost(&home)
            .args(["api", "/organizations", "-X", method])
            .assert()
            .code(2)
            .stderr(predicate::str::contains("unknown method"));
    }
}

/// The endpoint is a path, never a full URL.
#[test]
fn full_urls_are_not_endpoints() {
    let home = TempHome::new();

    selfhost(&home)
        .args(["api", "https://api.selfhost.dev/organizations"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("not a full URL"));
}

/// `..` segments would escape the API root, so they are refused up front.
#[test]
fn traversal_segments_are_rejected() {
    let home = TempHome::new();

    for endpoint in ["/a/../b", ".."] {
        selfhost(&home)
            .args(["api", endpoint])
            .assert()
            .code(2)
            .stderr(predicate::str::contains(".."));
    }
}

/// Both field flags take `KEY=VALUE`; anything else stops before a request.
#[test]
fn fields_need_key_value_form() {
    let home = TempHome::new();

    for flag in ["-f", "-F"] {
        selfhost(&home)
            .args(["api", "/organizations", flag, "bare"])
            .assert()
            .code(2)
            .stderr(predicate::str::contains("KEY=VALUE"));
    }
}

/// The headers the CLI manages fail the whole command instead of being
/// overridden or silently dropped — whatever the case.
#[test]
fn managed_headers_fail_the_command() {
    let home = TempHome::new();

    for header in [
        "Authorization: x",
        "authorization: x",
        "Accept: x",
        "Content-Type: application/json",
        "content-length: 3",
    ] {
        selfhost(&home)
            .args(["api", "/organizations", "-H", header])
            .assert()
            .code(2)
            .stderr(predicate::str::contains("managed by the CLI"));
    }
}

/// Headers split on the first colon; without one (or without a name) the run
/// stops before any credential is read.
#[test]
fn headers_need_name_value_form() {
    let home = TempHome::new();

    for header in ["no-colon", ": value"] {
        selfhost(&home)
            .args(["api", "/organizations", "-H", header])
            .assert()
            .code(2)
            .stderr(predicate::str::contains("NAME: VALUE"));
    }
}

/// `{org}` with nothing selected anywhere stops with the hint, before a
/// credential is needed and without a request — like every org-scoped command.
#[test]
fn the_org_placeholder_without_an_organization_names_the_fix() {
    let home = TempHome::new();

    selfhost(&home)
        .args(["api", "/organizations/{org}/members"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains(NO_ORGANIZATION));
}

/// A scoped path with nothing selected anywhere stops with the hint, before a
/// credential is needed and without a request (issue #8).
#[test]
fn a_scoped_path_without_an_organization_names_the_fix() {
    let home = TempHome::new();

    selfhost(&home)
        .args(["api", "/api/v1/platform/projects"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains(NO_ORGANIZATION));
}

/// Fields do not change the guard: a scoped GET carrying `-f` pairs with
/// nothing selected stops with the hint before any credential is read.
#[test]
fn a_scoped_get_with_fields_without_an_organization_names_the_fix() {
    let home = TempHome::new();

    selfhost(&home)
        .args(["api", "/v1/postgres", "-f", "limit=5"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains(NO_ORGANIZATION));
}

/// `--dry-run` fails closed with the shared refusal, before any credential is
/// read or request is sent.
#[test]
fn a_dry_run_sends_nothing() {
    let home = TempHome::new();

    selfhost(&home)
        .args(["api", "/organizations", "--dry-run"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("nothing was sent"));
}

/// An unknown profile is a usage error, like every other command.
#[test]
fn an_unknown_profile_is_a_usage_error() {
    let home = TempHome::new();

    selfhost(&home)
        .args(["--profile", "nope", "api", "/organizations"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("unknown profile 'nope'"));
}

/// The endpoint positional is required.
#[test]
fn a_missing_endpoint_is_a_usage_error() {
    let home = TempHome::new();

    selfhost(&home)
        .arg("api")
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("ENDPOINT"));
}

/// Without credentials the run stops at the sign-in hint (exit 3), like the
/// org verbs — profile and auth handling are unchanged.
#[test]
fn an_unsigned_in_run_stops_at_the_sign_in_hint() {
    let home = TempHome::new();

    selfhost(&home)
        .args(["api", "/organizations"])
        .assert()
        .code(3)
        .stderr(predicate::str::contains("selfhost auth login"));
}

/// `--silent` prints nothing on stdout; the exit code still says how it went.
#[test]
fn silent_leaves_stdout_empty() {
    let home = TempHome::new();

    let assert = selfhost(&home)
        .args(["api", "/organizations", "--silent"])
        .assert()
        .code(3);
    assert.stdout(predicate::str::is_empty().from_utf8());
}

/// The escape hatch documents every knob on its own help page.
#[test]
fn help_documents_every_api_flag() {
    let home = TempHome::new();

    let assert = selfhost(&home).args(["api", "--help"]).assert().success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    for flag in [
        "--method",
        "--raw-field",
        "--field",
        "--input",
        "--header",
        "--include",
        "--silent",
    ] {
        assert!(
            stdout.contains(flag),
            "`selfhost api --help` does not document {flag}:\n{stdout}"
        );
    }
}
