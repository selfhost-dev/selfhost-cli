//! End-to-end tests for the sign-in flow and the profile store behind it.
//!
//! Every test runs against a private `HOME`, so the store lands in a temp
//! directory and no test touches the developer's real `~/.selfhost`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use assert_cmd::Command;
use predicates::prelude::*;

/// Names the seeded profiles a fresh home starts with.
const SEEDED: [&str; 4] = ["default", "local", "prod", "qa"];

/// A throwaway home directory, `0700`, removed on drop (best effort).
struct TempHome {
    path: PathBuf,
}

impl TempHome {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("selfhost-flow-{}-{unique}", std::process::id()));
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

    /// `$HOME/.selfhost/config.json`.
    fn store_file(&self) -> PathBuf {
        self.path.join(".selfhost").join("config.json")
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

/// Parse a JSON array printed on stdout.
fn json_rows(stdout: &str) -> Vec<serde_json::Value> {
    serde_json::from_str::<Vec<serde_json::Value>>(stdout)
        .unwrap_or_else(|err| panic!("stdout is a JSON array ({err}):\n{stdout}"))
}

/// The row whose `check` is `name`.
fn row<'a>(rows: &'a [serde_json::Value], name: &str) -> &'a serde_json::Value {
    rows.iter()
        .find(|row| row.get("check").and_then(serde_json::Value::as_str) == Some(name))
        .unwrap_or_else(|| panic!("a '{name}' row: {rows:?}"))
}

#[test]
fn fresh_home_reports_not_signed_in() {
    let home = TempHome::new();

    let status = selfhost(&home)
        .args(["auth", "status", "--format", "json"])
        .assert()
        .code(3);
    let stdout = String::from_utf8_lossy(&status.get_output().stdout).into_owned();
    let rows = json_rows(&stdout);
    assert_eq!(
        row(&rows, "credentials").get("level"),
        Some(&serde_json::Value::String("fail".to_string())),
        "a fresh home has no credentials: {rows:?}"
    );

    selfhost(&home)
        .args(["auth", "token"])
        .assert()
        .code(3)
        .stderr(predicate::str::contains("selfhost auth login"));
}

#[test]
fn seeded_profiles_exist_before_the_store_file_is_written() {
    let home = TempHome::new();

    let assert = selfhost(&home)
        .args(["profile", "list", "--format", "json"])
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    let rows = json_rows(&stdout);

    for name in SEEDED {
        assert!(
            rows.iter()
                .any(|row| row.get("name").and_then(serde_json::Value::as_str) == Some(name)),
            "'{name}' must be seeded: {rows:?}"
        );
    }
    let default = rows
        .iter()
        .find(|row| row.get("name").and_then(serde_json::Value::as_str) == Some("default"))
        .expect("a default row");
    assert_eq!(default.get("default"), Some(&serde_json::Value::Bool(true)));

    assert!(
        !home.store_file().exists(),
        "listing must not create the store file"
    );
}

#[test]
fn profile_use_switches_the_default_and_writes_a_locked_down_store() {
    let home = TempHome::new();

    selfhost(&home)
        .args(["profile", "use", "qa"])
        .assert()
        .success();

    let file = home.store_file();
    assert!(file.exists(), "profile use must write the store");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let file_mode = std::fs::metadata(&file)
            .expect("the store exists")
            .permissions()
            .mode();
        assert_eq!(file_mode & 0o777, 0o600, "the store file must be private");
        let dir = file.parent().expect("the store has a parent");
        let dir_mode = std::fs::metadata(dir)
            .expect("the store dir exists")
            .permissions()
            .mode();
        assert_eq!(dir_mode & 0o777, 0o700, "the store dir must be private");
    }

    let assert = selfhost(&home)
        .args(["profile", "list", "--format", "json"])
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    let rows = json_rows(&stdout);
    let qa = rows
        .iter()
        .find(|row| row.get("name").and_then(serde_json::Value::as_str) == Some("qa"))
        .expect("a qa row");
    assert_eq!(qa.get("default"), Some(&serde_json::Value::Bool(true)));
}

#[test]
fn unknown_profile_is_refused_with_the_create_hint() {
    let home = TempHome::new();

    selfhost(&home)
        .args(["--profile", "nope", "auth", "login"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("profile add nope"));
}

#[test]
fn profile_add_requires_and_validates_the_base_url() {
    let home = TempHome::new();

    // No URL anywhere: a usage error.
    selfhost(&home)
        .args(["profile", "add", "staging"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("--base-url"));

    // Plain http only survives for localhost.
    selfhost(&home)
        .args([
            "profile",
            "add",
            "staging",
            "--base-url",
            "http://api.example.com",
        ])
        .assert()
        .code(1);

    selfhost(&home)
        .args([
            "profile",
            "add",
            "staging",
            "--base-url",
            "https://api.example.com",
        ])
        .assert()
        .success();

    // The name is taken now.
    selfhost(&home)
        .args([
            "profile",
            "add",
            "staging",
            "--base-url",
            "https://api.example.com",
        ])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("already exists"));
}

#[test]
fn profile_add_derives_the_console_for_known_hosts() {
    let home = TempHome::new();

    selfhost(&home)
        .args([
            "profile",
            "add",
            "qaish",
            "--base-url",
            "https://qapi.selfhost.dev",
        ])
        .assert()
        .success();
    let assert = selfhost(&home)
        .args(["profile", "show", "qaish", "--format", "json"])
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    let shown: serde_json::Value = serde_json::from_str(&stdout).expect("a JSON object");
    assert_eq!(shown.get("console_url"), Some(&serde_json::Value::Null));

    selfhost(&home)
        .args([
            "profile",
            "add",
            "staging",
            "--base-url",
            "https://api.staging.example.com",
        ])
        .assert()
        .success();
    let assert = selfhost(&home)
        .args(["profile", "show", "staging", "--format", "json"])
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    let shown: serde_json::Value = serde_json::from_str(&stdout).expect("a JSON object");
    assert_eq!(shown.get("console_url"), Some(&serde_json::Value::Null));
}

#[test]
fn default_profile_cannot_be_removed() {
    let home = TempHome::new();

    selfhost(&home)
        .args(["profile", "remove", "default"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("profile use"));
}

#[test]
fn environment_selects_the_profile_before_the_stored_default() {
    let home = TempHome::new();

    let assert = selfhost(&home)
        .env("SELFHOSTDEV_PROFILE", "qa")
        .args(["auth", "status", "--format", "json"])
        .assert()
        .code(3);
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    let rows = json_rows(&stdout);

    let profile = row(&rows, "profile");
    let detail = profile
        .get("detail")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    assert!(
        detail.contains("QA") && detail.contains("selected"),
        "the env var must select qa, not the stored default: {detail:?}"
    );
}

#[test]
fn login_on_the_local_profile_names_the_missing_console_url() {
    let home = TempHome::new();

    selfhost(&home)
        .args(["auth", "login", "--profile", "local"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("console_url"));
}

#[test]
fn logout_on_an_unsigned_profile_is_a_clean_no_op() {
    let home = TempHome::new();

    selfhost(&home).args(["auth", "logout"]).assert().success();
}

#[cfg(unix)]
#[test]
fn loose_store_permissions_are_tightened_before_use() {
    use std::os::unix::fs::PermissionsExt as _;

    let home = TempHome::new();
    selfhost(&home)
        .args(["profile", "use", "qa"])
        .assert()
        .success();

    let file = home.store_file();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644))
        .expect("the mode is loosened for the test");

    selfhost(&home).args(["profile", "list"]).assert().success();

    let mode = std::fs::metadata(&file)
        .expect("the store exists")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600, "loading must repair loose permissions");
}

#[test]
fn base_url_override_is_validated_before_any_request() {
    let home = TempHome::new();

    // A cleartext override must fail before a request is ever built.
    selfhost(&home)
        .args(["--base-url", "http://cleartext.example", "auth", "status"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("https"));

    // ...and before a browser is opened by `auth login`.
    selfhost(&home)
        .args(["auth", "login", "--base-url", "http://cleartext.example"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("https"));
}

#[test]
fn profile_add_refuses_userinfo_in_the_base_url() {
    let home = TempHome::new();

    selfhost(&home)
        .args([
            "profile",
            "add",
            "x",
            "--base-url",
            "http://localhost:3000@evil.com",
        ])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("user info"));

    // A case-insensitive https scheme is still accepted.
    selfhost(&home)
        .args(["profile", "add", "x", "--base-url", "HTTPS://api.selfhost.dev"])
        .assert()
        .success();
}

#[test]
fn empty_profile_selector_and_base_url_fall_back_to_the_default() {
    let home = TempHome::new();

    // An empty selector means "not set": the stored default is used.
    let assert = selfhost(&home)
        .env("SELFHOSTDEV_PROFILE", "")
        .args(["auth", "status", "--format", "json"])
        .assert()
        .code(3);
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    let rows = json_rows(&stdout);
    let detail = row(&rows, "profile")
        .get("detail")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    assert!(detail.contains("DEFAULT"), "{detail}");

    // Same for an explicit empty flag; an empty --base-url is also "unset", so
    // the row shows the profile's own endpoint.
    let assert = selfhost(&home)
        .args(["--profile", "", "--base-url", "", "auth", "status", "--format", "json"])
        .assert()
        .code(3);
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    let rows = json_rows(&stdout);
    assert_eq!(
        row(&rows, "api").get("detail"),
        Some(&serde_json::Value::String(
            "https://api.selfhost.dev".to_string()
        ))
    );
}

#[test]
fn profile_signed_in_ignores_the_environment_credentials() {
    let home = TempHome::new();

    let assert = selfhost(&home)
        .env("FIREBASE_API_KEY", "env-key")
        .env("FIREBASE_REFRESH_TOKEN", "env-token")
        .args(["profile", "list", "--format", "json"])
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    let rows = json_rows(&stdout);

    for row in &rows {
        assert_eq!(
            row.get("signed_in"),
            Some(&serde_json::Value::Bool(false)),
            "environment credentials are not profile state: {row:?}"
        );
    }
}

#[cfg(unix)]
#[test]
fn an_existing_loose_store_directory_is_tightened_on_save() {
    use std::os::unix::fs::PermissionsExt as _;

    let home = TempHome::new();
    let dir = home.path().join(".selfhost");
    std::fs::create_dir_all(&dir).expect("the store dir is pre-created");
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755))
        .expect("the mode is loosened for the test");

    selfhost(&home)
        .args(["profile", "use", "qa"])
        .assert()
        .success();

    let mode = std::fs::metadata(&dir)
        .expect("the store dir exists")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o700, "an existing ~/.selfhost must be 0700");
}

#[cfg(unix)]
#[test]
fn save_refuses_a_symlinked_store_directory() {
    let home = TempHome::new();
    let real = home.path().join("elsewhere");
    std::fs::create_dir_all(&real).expect("the target dir is created");
    std::os::unix::fs::symlink(&real, home.path().join(".selfhost"))
        .expect("the store dir is symlinked");

    selfhost(&home)
        .args(["profile", "use", "qa"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("symbolic link"));

    assert!(
        !real.join("config.json").exists(),
        "nothing may be written through the symlinked directory"
    );
}
