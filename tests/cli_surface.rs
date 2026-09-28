//! Surface tests for the Slice 0 scaffold: the `--help` group list, the command
//! tree, the `not implemented yet` staging error, completions and version.

use assert_cmd::Command;
use predicates::prelude::*;

/// Every top-level group documented in design §3.
const TOP_LEVEL_GROUPS: &[&str] = &[
    "auth",
    "profile",
    "config",
    "org",
    "project",
    "deploy",
    "github",
    "domain",
    "postgres",
    "mysql",
    "mongo",
    "redis",
    "clickhouse",
    "opensearch",
    "catalog",
    "billing",
    "cloud",
    "network",
    "ssh-key",
    "alert",
    "scaling",
    "webhook",
    "help",
    "tree",
    "completion",
];

fn selfhost() -> Command {
    Command::cargo_bin("selfhost").expect("the selfhost binary is built")
}

/// Group names as listed in clap's subcommand section: lines indented by two spaces.
fn listed_commands(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .filter_map(|line| line.strip_prefix("  "))
        .filter_map(|line| line.split_whitespace().next())
        .map(str::to_string)
        .collect()
}

#[test]
fn help_lists_every_top_level_group() {
    let assert = selfhost().arg("--help").assert().success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    let listed = listed_commands(&stdout);

    for group in TOP_LEVEL_GROUPS {
        assert!(
            listed.iter().any(|listed| listed == group),
            "`--help` does not list the `{group}` group:\n{stdout}"
        );
    }
}

#[test]
fn tree_prints_the_command_tree() {
    selfhost()
        .arg("tree")
        .assert()
        .success()
        .stdout(predicate::str::contains("selfhost postgres users"))
        .stdout(predicate::str::contains("selfhost postgres users list"))
        .stdout(predicate::str::contains(
            "selfhost opensearch dashboards disable",
        ));
}

#[test]
fn unimplemented_commands_report_their_full_path() {
    selfhost()
        .args(["org", "list"])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("not implemented yet: org list"));

    selfhost()
        .args(["postgres", "users", "rotate-password", "awsinst_1", "app"])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains(
            "not implemented yet: postgres users rotate-password",
        ));
}

#[test]
fn completion_emits_a_script() {
    selfhost()
        .args(["completion", "bash"])
        .assert()
        .success()
        .stdout(predicate::str::contains("selfhost"));

    selfhost()
        .args(["completion", "zsh"])
        .assert()
        .success()
        .stdout(predicate::str::contains("#compdef selfhost"));
}

#[test]
fn help_subcommand_matches_the_flag() {
    selfhost()
        .args(["help", "postgres"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Usage: selfhost postgres <COMMAND>",
        ));

    selfhost()
        .args(["help", "postgres", "users"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Usage: selfhost postgres users <COMMAND>",
        ));

    selfhost()
        .args(["help", "does-not-exist"])
        .assert()
        .failure()
        .code(2);
}

#[test]
fn version_is_reported() {
    selfhost()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));
}
