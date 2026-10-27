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
        vec!["org", "members", "add", "dana@example.com"],
        vec!["org", "activity", "list"],
        vec!["org", "invites", "list"],
        vec!["org", "invites", "create", "dana@example.com"],
        vec!["org", "roles", "list"],
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

/// A destructive verb asks before it acts. With no terminal and no `--yes`
/// there is nobody to ask, so it stops with the exact fix instead of hanging.
/// The gate is first, before any credential — which is why these exit 2
/// (usage), not 3 — and no request is made.
#[test]
fn destructive_org_commands_without_yes_stop_at_the_confirmation_hint() {
    let home = TempHome::new();

    for (command, hint) in [
        (
            vec!["org", "delete", "acme"],
            "org delete needs confirmation; pass --yes to run it non-interactively",
        ),
        (
            vec!["org", "members", "remove", "dana@example.com"],
            "org members remove needs confirmation; pass --yes to run it non-interactively",
        ),
        (
            vec!["org", "invites", "revoke", "dana@example.com"],
            "org invites revoke needs confirmation; pass --yes to run it non-interactively",
        ),
    ] {
        selfhost(&home)
            .args(&command)
            .assert()
            .code(2)
            .stderr(predicate::str::contains(hint));
    }
}

/// `--yes` satisfies the gate, so the command moves on. `org delete acme` then
/// stops at the credential check (exit 3); the two verbs that name no
/// organization stop at the hint telling the user to select one (exit 2).
/// Neither is a confirmation error, and neither sent a request.
#[test]
fn yes_satisfies_the_destructive_gate_before_credentials_are_read() {
    let home = TempHome::new();

    selfhost(&home)
        .args(["org", "delete", "--yes", "acme"])
        .assert()
        .code(3)
        .stderr(predicate::str::contains("selfhost auth login"))
        .stderr(predicate::str::contains("needs confirmation").not());

    for command in [
        vec!["org", "members", "remove", "--yes", "dana@example.com"],
        vec!["org", "invites", "revoke", "--yes", "dana@example.com"],
    ] {
        selfhost(&home)
            .args(&command)
            .assert()
            .code(2)
            .stderr(predicate::str::contains(NO_ORGANIZATION))
            .stderr(predicate::str::contains("needs confirmation").not());
    }
}

/// The role list is the platform's now, so it needs a session and an
/// organization like every other listing instead of printing a table built into
/// this binary.
#[test]
fn roles_list_is_read_from_the_platform_and_needs_a_session() {
    let home = TempHome::new();

    selfhost(&home)
        .args(["profile", "set", "default", "org", "org_a1"])
        .assert()
        .success();

    selfhost(&home)
        .args(["org", "roles", "list"])
        .assert()
        .code(3)
        .stderr(predicate::str::contains("selfhost auth login"));
}

/// `--role` takes any slug: a role this CLI has never shipped reaches the
/// platform instead of stopping at a usage error.
#[test]
fn the_role_flag_accepts_a_role_the_cli_has_never_shipped() {
    let home = TempHome::new();

    for command in [
        vec![
            "org",
            "members",
            "add",
            "dana@example.com",
            "--role",
            "support_engineer",
        ],
        vec![
            "org",
            "invites",
            "create",
            "dana@example.com",
            "--role",
            "data_viewer",
        ],
        vec![
            "org",
            "members",
            "update-role",
            "dana@example.com",
            "--role",
            "editor",
        ],
    ] {
        selfhost(&home)
            .args(&command)
            .assert()
            .code(2)
            .stderr(predicate::str::contains(NO_ORGANIZATION))
            .stderr(predicate::str::contains("invalid value").not());
    }

    // Required where the verb changes an existing member's role...
    selfhost(&home)
        .args(["org", "members", "update-role", "dana@example.com"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("--role"));

    // ...optional where it is part of an invitation, which defaults to member,
    // so that run gets as far as the missing organization.
    selfhost(&home)
        .args(["org", "members", "add", "dana@example.com"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("--role").not())
        .stderr(predicate::str::contains(NO_ORGANIZATION));
}

/// `--slug` was never an API field — the platform derives the slug from the
/// name — so it is gone rather than silently ignored.
#[test]
fn create_no_longer_accepts_a_slug() {
    let home = TempHome::new();

    selfhost(&home)
        .args(["org", "create", "Acme", "--slug", "acme"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("--slug"));
}

/// `org update` with nothing to change is a usage error, decided before any
/// credential is read or organization resolved.
#[test]
fn update_without_a_field_to_change_is_a_usage_error() {
    let home = TempHome::new();

    for command in [
        vec!["org", "update"],
        vec!["org", "update", "acme"],
        vec!["org", "update", "--org", ""],
    ] {
        selfhost(&home)
            .args(&command)
            .assert()
            .code(2)
            .stderr(predicate::str::contains(
                "nothing to update: pass --name or --description",
            ));
    }
}

/// `--dry-run` on a mutating verb fails closed rather than quietly sending the
/// request: the global flag exists, but no organization change simulates it
/// yet, and sending anyway would be a false safety net. The gate sits before
/// the confirmation gate and before any credential, so these exit 2 (usage),
/// not 3 — and no request is made.
#[test]
fn a_dry_run_on_a_mutating_org_command_sends_nothing() {
    let home = TempHome::new();

    for command in [
        vec!["org", "create", "Acme", "--dry-run"],
        vec!["org", "update", "--dry-run", "--name", "Beta"],
        vec!["org", "delete", "acme", "--dry-run", "--yes"],
        vec!["org", "members", "add", "dana@example.com", "--dry-run"],
        vec!["org", "members", "remove", "dana@example.com", "--dry-run"],
        vec![
            "org",
            "members",
            "update-role",
            "dana@example.com",
            "--role",
            "admin",
            "--dry-run",
        ],
        vec!["org", "invites", "create", "dana@example.com", "--dry-run"],
        vec!["org", "invites", "revoke", "dana@example.com", "--dry-run"],
    ] {
        selfhost(&home)
            .args(&command)
            .assert()
            .code(2)
            .stderr(predicate::str::contains(
                "dry runs are not supported for organization changes yet; nothing was sent",
            ))
            .stderr(predicate::str::contains("needs confirmation").not());
    }
}
