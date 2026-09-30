//! End-to-end tests for `--dry-run`: the flag stops every command before it can
//! change anything or reach the network.
//!
//! Every test runs against a private `HOME`, so the store lands in a temp
//! directory and no test touches the developer's real `~/.selfhost`. Nothing
//! here needs credentials or a network either: the refusal happens in the
//! shared dispatch path, before a handler reads a profile or opens a socket.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use assert_cmd::Command;
use predicates::prelude::*;

/// The refusal a command with no subject of its own answers `--dry-run` with.
const REFUSED: &str = "dry runs are not supported for this command yet; nothing was sent";

/// The refusal the organization commands answer `--dry-run` with.
const ORG_REFUSED: &str =
    "dry runs are not supported for organization changes yet; nothing was sent";

/// A throwaway home directory, `0700`, removed on drop (best effort).
struct TempHome {
    path: PathBuf,
}

impl TempHome {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("selfhost-dry-{}-{unique}", std::process::id()));
        fs::create_dir_all(&path).expect("the temp home is created");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700))
                .expect("the temp home is private");
        }
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    /// `$HOME/.selfhost/config.json`.
    fn store_file(&self) -> PathBuf {
        self.path.join(".selfhost").join("config.json")
    }

    /// The store file as bytes, or `None` when nothing is on disk.
    fn store_bytes(&self) -> Option<Vec<u8>> {
        fs::read(self.store_file()).ok()
    }
}

impl Drop for TempHome {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
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

/// The rows `profile list --format json` prints, in order.
fn profile_rows(home: &TempHome) -> Vec<serde_json::Value> {
    let assert = selfhost(home)
        .args(["profile", "list", "--format", "json"])
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    serde_json::from_str(&stdout)
        .unwrap_or_else(|err| panic!("stdout is a JSON array ({err}):\n{stdout}"))
}

/// The named row of `profile list`.
fn profile_row(home: &TempHome, name: &str) -> serde_json::Value {
    profile_rows(home)
        .into_iter()
        .find(|row| row.get("name").and_then(serde_json::Value::as_str) == Some(name))
        .unwrap_or_else(|| panic!("a profile named '{name}'"))
}

/// Whether the named profile still carries stored credentials.
fn signed_in(home: &TempHome, name: &str) -> bool {
    profile_row(home, name)
        .get("signed_in")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
}

/// `profile add --dry-run` is refused before any store I/O: no profile is
/// created, nothing is written, and the refusal names what the flag did.
#[test]
fn profile_add_with_a_dry_run_creates_no_profile() {
    let home = TempHome::new();

    selfhost(&home)
        .args([
            "profile",
            "add",
            "probe",
            "--base-url",
            "https://api.selfhost.dev",
            "--dry-run",
        ])
        .assert()
        .code(2)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains(REFUSED));

    assert!(
        !home.store_file().exists(),
        "a refused profile add must not write the store"
    );
    assert!(
        profile_rows(&home)
            .iter()
            .all(|row| row.get("name").and_then(serde_json::Value::as_str) != Some("probe")),
        "the refusal still created the profile"
    );
}

/// `profile remove`, `profile use` and `profile set` under `--dry-run` leave the
/// store file byte for byte as it was.
#[test]
fn profile_changes_with_a_dry_run_leave_the_store_untouched() {
    let home = TempHome::new();
    selfhost(&home)
        .args(["profile", "set", "prod", "provider", "hetzner"])
        .assert()
        .success();
    let seeded = home
        .store_bytes()
        .expect("the seeding command wrote the store");

    for command in [
        vec!["profile", "remove", "prod", "--dry-run"],
        vec!["profile", "use", "qa", "--dry-run"],
        vec!["profile", "set", "prod", "org", "acme", "--dry-run"],
        vec![
            "profile",
            "set",
            "prod",
            "base_url",
            "https://qa.example.test",
            "--dry-run",
        ],
    ] {
        selfhost(&home)
            .args(&command)
            .assert()
            .code(2)
            .stdout(predicate::str::is_empty())
            .stderr(predicate::str::contains(REFUSED));
        assert_eq!(
            home.store_bytes().as_deref(),
            Some(seeded.as_slice()),
            "{} changed the store file",
            command.join(" ")
        );
    }

    // The values those runs would have changed are still the stored ones.
    assert_eq!(profile_row(&home, "prod")["provider"], "hetzner");
    assert!(profile_row(&home, "prod")["org"].is_null());
    assert_eq!(profile_row(&home, "default")["default"], true);
}

/// `auth logout --dry-run` leaves the stored credentials in place.
#[test]
fn auth_logout_with_a_dry_run_keeps_the_stored_credentials() {
    let home = TempHome::new();
    let store = home.store_file();
    fs::create_dir_all(store.parent().expect("the store has a directory"))
        .expect("the store directory is created");
    let config = serde_json::json!({
        "version": 1,
        "default_profile": "default",
        "profiles": {
            "default": {
                "base_url": "https://api.selfhost.dev",
                "firebase_api_key": "test-api-key",
                "firebase_refresh_token": "test-refresh-token",
            }
        }
    });
    fs::write(
        &store,
        serde_json::to_vec_pretty(&config).expect("the config serializes"),
    )
    .expect("the store is seeded");

    // The credentials are there to be lost: the comparison below means nothing
    // on a profile that was never signed in.
    assert!(
        signed_in(&home, "default"),
        "the seeded profile starts signed in"
    );
    let seeded = home.store_bytes().expect("the seeded store is on disk");

    selfhost(&home)
        .args(["auth", "logout", "--dry-run"])
        .assert()
        .code(2)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains(REFUSED));

    assert_eq!(
        home.store_bytes().as_deref(),
        Some(seeded.as_slice()),
        "auth logout changed the store file"
    );
    assert!(signed_in(&home, "default"), "the credentials were cleared");
}

/// The refusal is shared, not written into each handler: a read-only command, a
/// command that changes the profile, a local command and a command whose group
/// is still staged all stop the same way, before their handler runs.
#[test]
fn commands_without_a_subject_refuse_a_dry_run() {
    let home = TempHome::new();

    for command in [
        vec!["auth", "status", "--dry-run"],
        vec!["profile", "list", "--dry-run"],
        vec!["catalog", "regions", "--dry-run"],
        vec!["postgres", "list", "--dry-run"],
        vec!["postgres", "users", "list", "--dry-run"],
        vec!["tree", "--dry-run"],
        vec!["completion", "bash", "--dry-run"],
    ] {
        selfhost(&home)
            .args(&command)
            .assert()
            .code(2)
            .stdout(predicate::str::is_empty())
            .stderr(predicate::str::contains(REFUSED))
            .stderr(predicate::str::contains("not implemented yet").not());
    }

    assert!(
        !home.path().join(".selfhost").exists(),
        "a refused run wrote the store"
    );
}

/// Every organization verb refuses `--dry-run` — the read-only ones included —
/// and the refusal names organization changes rather than the flag alone.
#[test]
fn organization_commands_refuse_a_dry_run_without_writing_the_store() {
    let home = TempHome::new();

    for command in [
        vec!["org", "list", "--dry-run"],
        vec!["org", "show", "--dry-run"],
        vec!["org", "members", "list", "--dry-run"],
        vec!["org", "roles", "list", "--dry-run"],
        vec!["org", "activity", "list", "--dry-run"],
        vec!["org", "invites", "list", "--dry-run"],
        vec!["org", "use", "acme", "--dry-run"],
    ] {
        selfhost(&home)
            .args(&command)
            .assert()
            .code(2)
            .stdout(predicate::str::is_empty())
            .stderr(predicate::str::contains(ORG_REFUSED));
    }

    assert!(
        !home.path().join(".selfhost").exists(),
        "a refused run wrote the store"
    );
}

/// A bare `selfhost --dry-run` is refused rather than opening the interactive
/// terminal UI. This is the path with no terminal on either stream; the
/// terminal path was checked by hand in a pty, where the same refusal appears
/// and the screen never opens.
#[test]
fn a_bare_invocation_with_a_dry_run_is_refused() {
    let home = TempHome::new();

    selfhost(&home)
        .args(["--dry-run"])
        .assert()
        .code(2)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains(REFUSED));

    assert!(
        !home.path().join(".selfhost").exists(),
        "a refused run wrote the store"
    );
}
