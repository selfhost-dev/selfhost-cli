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
    "tui",
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

/// Run `<path> --help` (or bare `--help` for the empty root path) and return
/// its stdout.
fn help_of(path: &str) -> String {
    let mut command = selfhost();
    if !path.is_empty() {
        command.args(path.split_whitespace());
    }
    let assert = command.arg("--help").assert().success();
    String::from_utf8_lossy(&assert.get_output().stdout).into_owned()
}

/// The `(name, about)` pairs of a help page's `Commands:` section. Wrapped
/// continuation lines are folded back into the about they belong to.
fn commands_with_about(stdout: &str) -> Vec<(String, String)> {
    let mut listed: Vec<(String, String)> = Vec::new();
    let mut in_commands = false;
    for line in stdout.lines() {
        if line.starts_with("Commands:") {
            in_commands = true;
            continue;
        }
        if !in_commands {
            continue;
        }
        let Some(entry) = line.strip_prefix("  ") else {
            // A blank line or the next section heading ends the list.
            if !line.trim().is_empty() || listed.is_empty() {
                in_commands = false;
            }
            continue;
        };
        let (name, about) = match entry.split_once(char::is_whitespace) {
            Some((name, about)) => (name, about.trim()),
            None => (entry.trim(), ""),
        };
        if name.is_empty() {
            // Indented continuation of the previous entry's about.
            if let Some(last) = listed.last_mut() {
                last.1.push(' ');
                last.1.push_str(entry.trim());
            }
            continue;
        }
        listed.push((name.to_string(), about.to_string()));
    }
    listed
}

/// Every registered command must carry a one-line, customer-readable `about`:
/// non-empty and free of backticked command names.
#[test]
fn every_command_has_a_customer_facing_about() {
    let mut missing: Vec<String> = Vec::new();
    let mut stack = vec![String::new()];

    while let Some(path) = stack.pop() {
        let stdout = help_of(&path);
        for (name, about) in commands_with_about(&stdout) {
            let child = format!("{path} {name}").trim().to_string();
            if about.is_empty() {
                missing.push(format!("selfhost {child}: empty about"));
            } else if about.contains('`') {
                missing.push(format!("selfhost {child}: backticked about: {about}"));
            }
            stack.push(child);
        }
    }

    assert!(
        missing.is_empty(),
        "help copy problems:\n{}",
        missing.join("\n")
    );
}

/// Each engine group advertises exactly the verbs the platform supports for it.
///
/// Verified against the platform API: `database_users` needs a
/// `user_management` script (`task_creation_service.rb:505`), config tuning is
/// `postgresql_configs`/`mysql_configs` only (`routes.rb:403-411`), PITR is
/// `supports_pitr?` (postgres + mysql), and pooling is one controller per engine
/// (`pgbouncer`/`proxysql`/`chproxy`).
const ENGINE_VERBS: &[(&str, &[&str])] = &[
    (
        "postgres",
        &[
            "list",
            "show",
            "create",
            "delete",
            "start",
            "stop",
            "reboot",
            "fork",
            "resize",
            "scale",
            "failover",
            "update",
            "wait",
            "logs",
            "stats",
            "metrics",
            "users",
            "config",
            "snapshots",
            "backups",
            "pitr",
            "pool",
            "extensions",
            "tls",
            "durability",
            "replicas",
        ],
    ),
    (
        "mysql",
        &[
            "list",
            "show",
            "create",
            "delete",
            "start",
            "stop",
            "reboot",
            "fork",
            "resize",
            "scale",
            "failover",
            "update",
            "wait",
            "logs",
            "stats",
            "metrics",
            "users",
            "config",
            "snapshots",
            "backups",
            "pitr",
            "pool",
        ],
    ),
    (
        "mongo",
        &[
            "list",
            "show",
            "create",
            "delete",
            "start",
            "stop",
            "reboot",
            "fork",
            "resize",
            "scale",
            "failover",
            "update",
            "wait",
            "logs",
            "stats",
            "metrics",
            "snapshots",
            "backups",
        ],
    ),
    (
        "redis",
        &[
            "list",
            "show",
            "create",
            "delete",
            "start",
            "stop",
            "reboot",
            "fork",
            "resize",
            "scale",
            "failover",
            "update",
            "wait",
            "logs",
            "stats",
            "metrics",
            "users",
            "snapshots",
            "backups",
        ],
    ),
    (
        "clickhouse",
        &[
            "list",
            "show",
            "create",
            "delete",
            "start",
            "stop",
            "reboot",
            "fork",
            "resize",
            "scale",
            "failover",
            "update",
            "wait",
            "logs",
            "stats",
            "metrics",
            "users",
            "snapshots",
            "backups",
            "pool",
        ],
    ),
    (
        "opensearch",
        &[
            "list",
            "show",
            "create",
            "delete",
            "start",
            "stop",
            "reboot",
            "fork",
            "resize",
            "scale",
            "failover",
            "update",
            "wait",
            "logs",
            "stats",
            "metrics",
            "users",
            "snapshots",
            "backups",
            "dashboards",
        ],
    ),
];

#[test]
fn engine_groups_expose_exactly_their_supported_verbs() {
    for (engine, expected) in ENGINE_VERBS {
        let stdout = help_of(engine);
        let mut actual: Vec<String> = commands_with_about(&stdout)
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        actual.sort();
        let mut expected: Vec<String> = expected.iter().map(|verb| verb.to_string()).collect();
        expected.sort();
        assert_eq!(
            actual, expected,
            "`selfhost {engine}` verb set drifted from the platform surface:\n{stdout}"
        );
    }
}

/// Pooling is one controller per engine, and each only routes
/// `show`/`create`/`update`/`destroy` (`config/routes.rb:382-384`): reloading the
/// pooler's user list is an internal agent task, so no `reload-users` verb exists.
#[test]
fn pool_groups_do_not_advertise_reload_users() {
    for engine in ["postgres", "mysql", "clickhouse"] {
        let stdout = help_of(&format!("{engine} pool"));
        let verbs: Vec<String> = commands_with_about(&stdout)
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        assert_eq!(
            verbs,
            ["show", "enable", "disable", "update"],
            "`selfhost {engine} pool` verb set drifted (reload-users has no route):\n{stdout}"
        );
    }
}

/// `--provider` must be discoverable on every engine's `create` and `list`.
#[test]
fn every_engine_create_and_list_offer_the_provider_flag() {
    for (engine, _) in ENGINE_VERBS {
        for verb in ["create", "list"] {
            let stdout = help_of(&format!("{engine} {verb}"));
            assert!(
                stdout.contains("--provider"),
                "`selfhost {engine} {verb}` does not offer --provider:\n{stdout}"
            );
        }
    }
}

/// The `--help` trailer may only claim verbs every engine group actually has.
#[test]
fn help_trailer_lists_only_shared_engine_verbs() {
    let root = help_of("");
    let trailer: Vec<String> = root
        .lines()
        .skip_while(|line| !line.starts_with("Every engine group supports:"))
        .skip(1)
        .take_while(|line| !line.trim().is_empty())
        .map(|line| line.trim().trim_end_matches(',').to_string())
        .flat_map(|line| {
            line.split(',')
                .map(|verb| verb.trim().to_string())
                .collect::<Vec<_>>()
        })
        .filter(|verb| !verb.is_empty())
        .collect();
    assert!(!trailer.is_empty(), "the trailer lists no verbs:\n{root}");

    for (engine, _) in ENGINE_VERBS {
        let verbs: Vec<String> = commands_with_about(&help_of(engine))
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        for verb in &trailer {
            assert!(
                verbs.contains(verb),
                "the trailer advertises `{verb}` but `selfhost {engine}` has no such command"
            );
        }
    }

    // …and it must not omit a verb that all six do have.
    let mut shared = commands_with_about(&help_of(ENGINE_VERBS[0].0))
        .into_iter()
        .map(|(name, _)| name)
        .collect::<Vec<_>>();
    for (engine, _) in &ENGINE_VERBS[1..] {
        let verbs: Vec<String> = commands_with_about(&help_of(engine))
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        shared.retain(|verb| verbs.contains(verb));
    }
    for verb in &shared {
        assert!(
            trailer.contains(verb),
            "the trailer omits `{verb}`, which every engine group supports:\n{root}"
        );
    }
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
fn tui_is_registered_and_requires_a_terminal() {
    // The command surface: `tui` is listed at the top level and carries a
    // one-line about (the recursive walk in `every_command_has_a_customer_facing_about`
    // also covers it).
    let root = help_of("");
    assert!(
        root.lines()
            .any(|line| line.trim_start().starts_with("tui ")),
        "`--help` does not list the `tui` command:\n{root}"
    );

    // assert_cmd pipes both streams, so this exercises the non-TTY guard: a
    // clean usage error and exit 2, never a UI waiting on input. The
    // interactive path is covered by the `should_launch_tui` unit matrix and
    // the `TestBackend` render test.
    selfhost()
        .arg("tui")
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains(
            "selfhost tui requires an interactive terminal",
        ));
}

#[test]
fn tui_rejects_an_unknown_view() {
    selfhost()
        .args(["tui", "--view", "nope"])
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("nope"));
}

#[test]
fn tui_help_lists_every_flag() {
    let stdout = help_of("tui");
    for flag in ["--view", "--read-only", "--refresh"] {
        assert!(
            stdout.contains(flag),
            "`selfhost tui --help` does not document {flag}:\n{stdout}"
        );
    }
}

/// A bare `selfhost` under a non-TTY (assert_cmd pipes both streams) keeps the
/// CLI's usage output and exits 2 — scripts and CI must never block on the TUI.
/// The interactive-TTY path (stdin/stdout TTY, TERM not dumb, SELFHOST_NO_TUI
/// unset) is pinned by the `should_launch_tui` unit matrix in `src/cli/tui.rs`,
/// which cannot be exercised through a piped child process.
#[test]
fn bare_invocation_without_a_tty_prints_usage() {
    selfhost()
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("Usage: selfhost"));

    // `--help` and `--version` always win over the TUI path.
    selfhost().arg("--help").assert().success();
    selfhost().arg("--version").assert().success();

    // Global flags alone are still "no subcommand" — same non-TTY outcome.
    selfhost()
        .args(["--profile", "qa"])
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("Usage: selfhost"));
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

/// `--env` is gone: profiles are the only context knob, so the help must not
/// advertise the flag or a `SELFHOST_ENV` env var.
#[test]
fn help_advertises_profiles_not_env() {
    let stdout = help_of("");
    assert!(
        !stdout.contains("--env"),
        "root help still mentions --env:\n{stdout}"
    );
    assert!(
        !stdout.contains("SELFHOST_ENV"),
        "root help still mentions SELFHOST_ENV:\n{stdout}"
    );
    assert!(
        stdout.contains("--profile") && stdout.contains("SELFHOST_PROFILE"),
        "root help must advertise --profile / SELFHOST_PROFILE:\n{stdout}"
    );
}

/// The removed flag is rejected as an unknown argument (usage error, exit 2)
/// rather than silently accepted.
#[test]
fn env_flag_is_rejected() {
    selfhost()
        .args(["--env", "qa", "postgres", "list"])
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("--env"));
}
