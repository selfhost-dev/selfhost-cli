//! End-to-end tests for the organization commands.
//!
//! Every test runs against a private `HOME`, so the store lands in a temp
//! directory and no test touches the developer's real `~/.selfhost`. Nothing
//! here needs credentials or a network — the paths that do (a signed-in
//! listing, a slug lookup) are proven by the live check that follows this
//! slice, and the resolver itself is unit-tested with canned payloads.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use assert_cmd::Command;
use predicates::prelude::*;

/// The usage error an org-scoped command raises when nothing selected one.
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
            std::env::temp_dir().join(format!("selfhost-org-{}-{unique}", std::process::id()));
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

/// Every org-scoped command refuses to guess: with nothing selected anywhere it
/// stops with the hint, before a credential is needed and without a request.
#[test]
fn org_commands_without_an_organization_report_the_use_hint() {
    let home = TempHome::new();

    for command in [
        vec!["org", "show"],
        vec!["org", "show", "--org", ""],
        vec!["org", "members", "list"],
        vec!["org", "activity", "list"],
    ] {
        selfhost(&home)
            .args(&command)
            .assert()
            .code(2)
            .stderr(predicate::str::contains(NO_ORGANIZATION));
    }
}

/// The listing is the first thing a signed-out user reaches; it says what to do
/// about it instead of leaking a transport failure.
#[test]
fn org_list_without_credentials_stops_at_exit_3() {
    let home = TempHome::new();

    selfhost(&home)
        .args(["org", "list"])
        .assert()
        .code(3)
        .stderr(predicate::str::contains("selfhost auth login"));
}

/// The empty flag and an empty environment variable are both "unset", so the
/// organization saved in the profile is used instead of the usage error.
#[test]
fn an_empty_org_reference_falls_back_to_the_profiles_organization() {
    let home = TempHome::new();

    selfhost(&home)
        .args(["profile", "set", "default", "org", "acme"])
        .assert()
        .success();

    for command in [
        selfhost(&home).args(["org", "show"]),
        selfhost(&home).args(["--org", "", "org", "show"]),
        selfhost(&home)
            .env("SELFHOSTDEV_ORG", "")
            .args(["org", "show"]),
    ] {
        command
            .assert()
            .code(3)
            .stderr(predicate::str::contains("selfhost auth login"))
            .stderr(predicate::str::contains(NO_ORGANIZATION).not());
    }
}
