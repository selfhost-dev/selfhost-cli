//! Command tree and dispatch (design §3, §4, §7).
//!
//! Slice 0 registers the complete documented surface as real clap subcommands and
//! stages the behaviour: commands whose implementation a later slice owns answer with
//! `not implemented yet: <full command path>`. `auth`, `profile` and the organization
//! commands are real, as are `tui`, `help`, `tree` and `completion`.
//!
//! * adding a command: one line in the group's [`stub_group!`] call;
//! * adding behaviour: a real arm in the group's dispatch, or a new group module.

use std::io::{IsTerminal as _, Write as _};

use anyhow::anyhow;
use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt as _, BufReader};

use crate::api::ApiClient;
use crate::config::{Profile, ProfileStore, validate_endpoint};
use crate::error::{Error, Result};
use crate::output::Format;

pub mod alert;
pub mod api;
pub mod auth;
pub mod billing;
pub mod catalog;
pub mod clickhouse;
pub mod cloud;
pub mod completion;
pub mod config;
pub mod deploy;
pub mod domain;
pub mod github;
pub mod help;
pub mod mongo;
pub mod mysql;
pub mod network;
pub mod opensearch;
pub mod org;
pub mod postgres;
pub mod profile;
pub mod project;
pub mod redis;
pub mod scaling;
pub mod ssh_key;
pub mod tree;
pub mod tui;
pub mod update;
pub mod webhook;

// Register a command group: its clap subcommands plus the Slice 0 staging error.
//
// Each variant either carries a leaf's argument struct or a nested group's command
// enum; `path()` is the full command path (`postgres users list`) and `dispatch()`
// returns [`crate::error::Error::NotImplemented`] carrying it — nested groups
// delegate down until a leaf answers. Replace a dispatch arm with real code in a
// later slice.
macro_rules! stub_group {
    (
        $(#[$meta:meta])*
        $cmd:ident, $prefix:literal,
        leaves { $( $(#[$leaf_meta:meta])* $leaf:ident ( $leaf_ty:ty ) => $leaf_name:literal ),* $(,)? }
        groups { $( $group:ident ( $group_ty:ty ) => $group_name:literal ),* $(,)? }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, clap::Subcommand)]
        pub enum $cmd {
            $(
                $(#[$leaf_meta])*
                #[command(name = $leaf_name)]
                $leaf($leaf_ty),
            )*
            $(
                #[command(name = $group_name)]
                #[command(subcommand)]
                $group($group_ty),
            )*
        }

        impl $cmd {
            /// Full command path, e.g. `postgres users list`.
            pub fn path(&self) -> &'static str {
                match self {
                    $( Self::$leaf(_) => concat!($prefix, " ", $leaf_name), )*
                    $( Self::$group(_) => concat!($prefix, " ", $group_name), )*
                }
            }

            /// Slice 0 staging: registered commands report their path and exit 1.
            pub fn dispatch(&self) -> crate::error::Result<()> {
                let path = self.path();
                $(
                    if let Self::$group(inner) = self {
                        return inner.dispatch();
                    }
                )*
                Err(crate::error::Error::not_implemented(path))
            }
        }
    };
}

pub(crate) use stub_group;

// Cloud a database instance is provisioned on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Provider {
    /// Amazon Web Services
    Aws,
    /// Hetzner Cloud
    Hetzner,
}

// Options accepted by every command, before or after the subcommand (design §3).
#[derive(Debug, Clone, Args)]
pub struct GlobalArgs {
    /// Saved profile to use
    #[arg(
        short = 'p',
        long,
        global = true,
        env = "SELFHOSTDEV_PROFILE",
        hide_env_values = true,
        value_name = "NAME",
        help_heading = "Global options"
    )]
    pub profile: Option<String>,

    /// API base URL (default: the profile's, or https://api.selfhost.dev)
    #[arg(
        long = "base-url",
        global = true,
        env = "SELFHOSTDEV_BASE_URL",
        hide_env_values = true,
        value_name = "URL",
        help_heading = "Global options"
    )]
    pub base_url: Option<String>,

    /// Organization slug or pid (default: the profile's organization)
    #[arg(
        long,
        global = true,
        env = "SELFHOSTDEV_ORG",
        hide_env_values = true,
        value_name = "SLUG|PID",
        help_heading = "Global options"
    )]
    pub org: Option<String>,

    /// How to print results (default: table in a terminal, json when piped)
    #[arg(
        short = 'o',
        long = "format",
        global = true,
        value_enum,
        value_name = "FMT",
        help_heading = "Global options"
    )]
    pub format: Option<Format>,

    /// Print JSON (same as --format json)
    #[arg(long, global = true, help_heading = "Global options")]
    pub json: bool,

    /// Turn off colored output
    #[arg(long = "no-color", global = true, help_heading = "Global options")]
    pub no_color: bool,

    /// Seconds to wait for one API request before giving up
    #[arg(
        long,
        global = true,
        default_value_t = 30,
        value_name = "SECS",
        help_heading = "Global options"
    )]
    pub timeout: u64,

    /// Seconds between progress checks while waiting for an operation
    #[arg(
        long = "poll-interval",
        global = true,
        value_name = "SECS",
        help_heading = "Global options"
    )]
    pub poll_interval: Option<u64>,

    /// Answer yes to every confirmation prompt
    #[arg(short = 'y', long, global = true, help_heading = "Global options")]
    pub yes: bool,

    /// Stop before doing anything: nothing is previewed or sent yet
    #[arg(long = "dry-run", global = true, help_heading = "Global options")]
    pub dry_run: bool,

    /// Print errors only
    #[arg(short = 'q', long, global = true, help_heading = "Global options")]
    pub quiet: bool,

    /// Show API requests as they are made
    #[arg(short = 'v', long, global = true, help_heading = "Global options")]
    pub verbose: bool,

    /// Show full request and response details (secrets hidden)
    #[arg(long, global = true, help_heading = "Global options")]
    pub debug: bool,

    // clap's built-in `-h`/`-V` flags have no heading of their own, which left
    // a two-entry `Options:` block above the global wall. These replace them
    // (the built-ins are disabled on the root) so every option in the help
    // lands in one `Global options` block.
    /// Print help
    #[arg(
        short = 'h',
        long = "help",
        global = true,
        action = clap::ArgAction::Help,
        help_heading = "Global options"
    )]
    pub help: Option<bool>,

    /// Print version
    #[arg(
        short = 'V',
        long = "version",
        action = clap::ArgAction::Version,
        help_heading = "Global options"
    )]
    pub version: Option<bool>,
}

/// The base URL this run should talk to: an explicit `--base-url` /
/// `SELFHOSTDEV_BASE_URL` (an empty value counts as unset) over the profile's
/// own. Whichever wins is validated before an [`crate::api::ApiClient`] is
/// built, so a bad override fails closed instead of being sent verbatim.
pub fn effective_base_url(global: &GlobalArgs, profile: &Profile) -> crate::error::Result<String> {
    let chosen = global
        .base_url
        .as_deref()
        .filter(|url| !url.is_empty())
        .unwrap_or(&profile.base_url);
    validate_endpoint("base_url", chosen)?;
    Ok(chosen.to_string())
}

/// The organization reference this run should resolve: a command's own
/// argument first, then an explicit `--org` / `SELFHOSTDEV_ORG` (an empty
/// value counts as unset, like `--base-url`), then the profile's stored org.
/// `None` means nothing selected one.
pub fn org_reference<'a>(
    explicit: Option<&'a str>,
    global: &'a GlobalArgs,
    profile: &'a Profile,
) -> Option<&'a str> {
    [explicit, global.org.as_deref(), profile.org.as_deref()]
        .into_iter()
        .flatten()
        .find(|reference| !reference.is_empty())
}

/// The usage error for an org-scoped command with no organization anywhere:
/// the profile has none, and neither `--org` nor the command named one.
pub fn no_organization_selected() -> Error {
    Error::Usage("no organization selected; run selfhost org use <slug>".to_string())
}

/// The usage error for a reference that is not one of the caller's
/// organizations.
pub fn unknown_organization(reference: &str) -> Error {
    Error::Usage(format!(
        "unknown organization '{reference}'; run selfhost org list"
    ))
}

/// The longest organization pid accepted: the server's `org_<hex>` values are
/// far shorter, so anything beyond this is hostile input, not a real pid.
const ORGANIZATION_PID_MAX: usize = 64;

/// Whether `reference` is already an organization pid (`org_<hex>`, within
/// [`ORGANIZATION_PID_MAX`]), which [`resolve_org_pid`] injects without a
/// lookup. Anything else is a slug.
pub fn is_org_pid(reference: &str) -> bool {
    reference.len() <= ORGANIZATION_PID_MAX
        && reference
            .strip_prefix("org_")
            .is_some_and(|hex| !hex.is_empty() && hex.bytes().all(|byte| byte.is_ascii_hexdigit()))
}

/// The pid an organization record carries: `pid` on the raw payloads, `id` on
/// the serialized membership payload. Only a value shaped like a pid is
/// accepted; anything else counts as no pid, so it never reaches a request path.
pub fn organization_pid(organization: &Value) -> Option<&str> {
    organization
        .get("pid")
        .and_then(Value::as_str)
        .or_else(|| organization.get("id").and_then(Value::as_str))
        .filter(|pid| is_org_pid(pid))
}

/// Whether an organization record is the one `reference` names, by pid or slug
/// — exactly, never as a prefix.
pub fn organization_matches(organization: &Value, reference: &str) -> bool {
    organization_pid(organization) == Some(reference)
        || organization.get("slug").and_then(Value::as_str) == Some(reference)
}

/// The organization named by `reference` in a `GET /organizations` payload,
/// matched by pid or slug — exactly, never as a prefix.
pub fn find_organization<'a>(organizations: &'a Value, reference: &str) -> Option<&'a Value> {
    organizations
        .as_array()?
        .iter()
        .find(|organization| organization_matches(organization, reference))
}

/// Resolve an organization reference to the pid to inject: a pid is used as-is,
/// anything else is an exact slug lookup against the caller's organizations
/// (the unscoped `GET /organizations`). An unknown reference is a usage error
/// naming the listing command.
pub async fn resolve_org_pid(client: &ApiClient, reference: &str) -> Result<String> {
    if is_org_pid(reference) {
        return Ok(reference.to_string());
    }
    let organizations = client.get_unscoped("/organizations", &[]).await?;
    let organization = find_organization(&organizations, reference)
        .ok_or_else(|| unknown_organization(reference))?;
    organization_pid(organization)
        .map(str::to_owned)
        .ok_or_else(|| Error::Other(anyhow!("organization '{reference}' has no pid")))
}

/// Run one async handler on a current-thread runtime.
pub fn block_on<F: Future<Output = Result<()>>>(future: F) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|err| {
            Error::Other(anyhow::Error::from(err).context("cannot start the async runtime"))
        })?;
    runtime.block_on(future)
}

/// Render a value in the resolved format on stdout.
pub fn print(global: &GlobalArgs, value: &Value) -> Result<()> {
    let format = Format::resolve(global.format, global.json);
    println!("{}", format.render(value)?);
    Ok(())
}

/// The client every command starts from: the selected profile's endpoint (or
/// the `--base-url` override), a fresh access token, and the request echo.
/// Organization injection stays off until [`org_client`].
pub async fn unscoped_client(
    store: &mut ProfileStore,
    name: &str,
    global: &GlobalArgs,
) -> Result<ApiClient> {
    let base_url = {
        let profile = store.require_profile(name)?;
        effective_base_url(global, profile)?
    };
    let token = crate::auth::id_token(store, name).await?;
    Ok(ApiClient::with_token(&base_url, token, global.timeout)
        .with_verbosity(global.verbose, global.debug))
}

/// The client an org-scoped command works against, plus the pid it resolved and
/// the reference it resolved from: [`unscoped_client`] with [`org_reference`]'s
/// organization injected into every scoped request, so `--org` /
/// `SELFHOSTDEV_ORG` (then the profile's stored org) reach any command that
/// inherits this path. `explicit` is the command's own positional argument,
/// which wins over both. The injected value is always the resolved pid, never
/// the slug. The reference comes back so a message can name the organization by
/// the name the user used. No reference anywhere fails before a credential is
/// needed.
pub async fn org_client(
    store: &mut ProfileStore,
    name: &str,
    global: &GlobalArgs,
    explicit: Option<&str>,
) -> Result<(ApiClient, String, String)> {
    let reference = {
        let profile = store.require_profile(name)?;
        org_reference(explicit, global, profile).map(str::to_owned)
    };
    let reference = reference.ok_or_else(no_organization_selected)?;
    let client = unscoped_client(store, name, global).await?;
    let pid = resolve_org_pid(&client, &reference).await?;
    Ok((client.with_org(Some(pid.clone())), pid, reference))
}

/// Whether this run prints for a person: the table format, which is the default
/// on a terminal. Commands that report one action print a single line for a
/// person and a curated object in JSON/YAML.
pub fn human_output(global: &GlobalArgs) -> bool {
    Format::resolve(global.format, global.json) == Format::Table
}

/// Whether a destructive command has to ask the user to confirm. `--yes`
/// answers the prompt up front, so a script says what it means; without it a run
/// that has no terminal on stdin fails with the exact fix instead of hanging.
/// The gate is the first thing such a command does, before a credential is read
/// or a request is sent (design §6).
pub fn should_confirm(global: &GlobalArgs, command: &str) -> Result<bool> {
    if global.yes {
        return Ok(false);
    }
    if std::io::stdin().is_terminal() {
        return Ok(true);
    }
    Err(Error::Usage(format!(
        "{command} needs confirmation; pass --yes to run it non-interactively"
    )))
}

/// The refusal `--dry-run` carries: the flag exists, but nothing simulates a
/// request yet, and a run that quietly sent the request anyway would be a false
/// safety net. The shared gate in [`Cli::run`] raises it for every command
/// before its handler runs — a handler must never send, write or read state for
/// a change while the flag is set — and the bare invocation in `main` raises it
/// for a run that never reached that gate.
pub fn dry_run_refused(subject: &str) -> Error {
    Error::Usage(format!(
        "dry runs are not supported for {subject} yet; nothing was sent"
    ))
}

/// [`dry_run_refused`] as a gate: nothing happens when the flag is unset.
pub fn reject_dry_run_for(global: &GlobalArgs, subject: &str) -> Result<()> {
    if global.dry_run {
        return Err(dry_run_refused(subject));
    }
    Ok(())
}

/// What `--dry-run` names when it refuses: nothing simulates a request yet, so
/// the refusal points at the closest thing to what the command would have
/// changed. Every command gets a subject, so the message can never claim a
/// subject the command does not have.
pub fn dry_run_subject_of_group(group: Option<&str>) -> &'static str {
    match group {
        Some("org") => "organization changes",
        Some("api") => "api calls",
        _ => "this command",
    }
}

/// Ask the user to type `expected` back. `prompt` is printed verbatim, so the
/// caller decides the wording. The answer counts only when it matches exactly
/// once trimmed — and an empty or whitespace-only `expected` never matches:
/// otherwise a bare Enter (or EOF, which reads as an empty line) would confirm.
pub async fn confirm_typed(prompt: &str, expected: &str) -> bool {
    let expected = expected.trim();
    if expected.is_empty() {
        return false;
    }
    eprint!("{prompt}");
    let _ = std::io::stderr().flush();
    read_confirmation().await.trim() == expected
}

/// Ask a yes/no question; only an explicit `y`/`yes` passes, so an interrupted
/// or empty answer means no.
pub async fn confirm_yes_no(question: &str) -> bool {
    eprint!("{question} [y/N] ");
    let _ = std::io::stderr().flush();
    let answer = read_confirmation().await;
    matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes")
}

/// One line of confirmation input; EOF and read errors read as an empty line,
/// which never confirms anything.
async fn read_confirmation() -> String {
    let mut answer = String::new();
    match BufReader::new(tokio::io::stdin())
        .read_line(&mut answer)
        .await
    {
        Ok(read) if read > 0 => answer,
        _ => String::new(),
    }
}

// A command with no documented flags yet.
#[derive(Debug, Clone, Args)]
pub struct NoArgs {}

// A single required positional: pid (`awsinst_*`), group id, or resource name.
#[derive(Debug, Clone, Args)]
pub struct TargetArgs {
    /// Database id, group id or name
    pub target: String,
}

// One optional positional — listings that can be scoped or left org-wide.
#[derive(Debug, Clone, Args)]
pub struct OptionalTargetArgs {
    /// Database id, group id or name
    pub target: Option<String>,
}

// A required instance pid, for the per-instance sub-resources.
#[derive(Debug, Clone, Args)]
pub struct PidArgs {
    /// Database id (awsinst_…)
    pub pid: String,
}

// A single required name.
#[derive(Debug, Clone, Args)]
pub struct NameArgs {
    /// Name
    pub name: String,
}

// `logs`, shared by engine instances, project databases and deploy runs.
#[derive(Debug, Clone, Args)]
pub struct LogsArgs {
    /// Instance pid or run pid
    pub pid: String,

    /// Log source (engine-specific, e.g. postgres or pgbouncer)
    #[arg(long)]
    pub source: Option<String>,

    /// Keep showing new output until it finishes
    #[arg(long)]
    pub follow: bool,

    /// How many lines to show
    #[arg(long)]
    pub lines: Option<u32>,

    /// Start from this time (timestamp or something like 1h)
    #[arg(long)]
    pub since: Option<String>,
}

// `restore`, shared by snapshots and backups.
#[derive(Debug, Clone, Args)]
pub struct RestoreArgs {
    /// Instance pid
    pub target: String,

    /// Snapshot/backup to restore from
    #[arg(long)]
    pub from: Option<String>,
}

// Activity-log style listing: `page`/`limit` plus an optional scope.
#[derive(Debug, Clone, Args)]
pub struct ActivityListArgs {
    /// Scope the listing (org/project/instance) when the endpoint allows it
    pub target: Option<String>,

    /// Page number
    #[arg(long)]
    pub page: Option<u32>,

    /// Page size
    #[arg(long)]
    pub limit: Option<u32>,

    /// Only entries after this time (RFC 3339 or relative)
    #[arg(long)]
    pub since: Option<String>,
}

// A custom-domain handle: `--domain` plus an optional owning resource.
#[derive(Debug, Clone, Args)]
pub struct DomainRefArgs {
    /// Owning resource (project, service, deployment)
    pub target: Option<String>,

    /// Domain name
    #[arg(long)]
    pub domain: Option<String>,
}

// SSH key material, shared by org keys and project keys.
#[derive(Debug, Clone, Args)]
pub struct SshKeyAddArgs {
    /// Key name
    pub name: Option<String>,

    /// Public key text (ssh-ed25519 AAAA…) or a path to a .pub file
    #[arg(long = "public-key")]
    pub public_key: Option<String>,

    /// Owning project
    #[arg(long)]
    pub project: Option<String>,
}

// `project ssh access set`: how a key may reach the project's instances.
#[derive(Debug, Clone, Args)]
pub struct AccessSetArgs {
    /// Project or instance to scope access to
    pub target: Option<String>,

    /// Access mode (all, bastion, read-only, …)
    #[arg(long)]
    pub mode: Option<String>,
}

// `list` for an engine group: the provider flag replaces Ruby's `hetzner` subgroup.
#[derive(Debug, Clone, Args)]
pub struct EngineListArgs {
    /// Cloud to list from
    #[arg(long, value_enum)]
    pub provider: Option<Provider>,

    /// Maximum number of instances to return
    #[arg(long)]
    pub limit: Option<u32>,
}

// `create`, shared by the engine groups (design §4).
#[derive(Debug, Clone, Args)]
pub struct EngineCreateArgs {
    /// Instance name
    #[arg(long)]
    pub name: Option<String>,

    /// Engine version, e.g. 16
    #[arg(long)]
    pub version: Option<String>,

    /// Region, e.g. us-east-1 or fsn1
    #[arg(long)]
    pub region: Option<String>,

    /// Instance type (see selfhost catalog instance-types)
    #[arg(long = "instance-type")]
    pub instance_type: Option<String>,

    /// Storage type (see selfhost catalog storage-types)
    #[arg(long = "storage-type")]
    pub storage_type: Option<String>,

    /// Storage size, e.g. 100gb
    #[arg(long)]
    pub size: Option<String>,

    /// Cloud to create the database on
    #[arg(long, value_enum)]
    pub provider: Option<Provider>,

    /// High availability (multi-node)
    #[arg(long)]
    pub ha: bool,

    /// Spread across availability zones
    #[arg(long = "multi-az")]
    pub multi_az: bool,

    /// Allocate a public endpoint
    #[arg(long)]
    pub public: bool,

    /// CIDR allowed to reach the instance (repeatable)
    #[arg(long = "allowed-cidr")]
    pub allowed_cidrs: Vec<String>,

    /// Block until provisioning finishes
    #[arg(long)]
    pub wait: bool,
}

// `postgres create`: the shared surface plus extensions to install.
#[derive(Debug, Clone, Args)]
pub struct PostgresCreateArgs {
    #[command(flatten)]
    pub base: EngineCreateArgs,

    /// Extension to install at create time (repeatable)
    #[arg(long)]
    pub extensions: Vec<String>,
}

// `clickhouse create`: the shared surface plus the ClickHouse storage mode.
#[derive(Debug, Clone, Args)]
pub struct ClickHouseCreateArgs {
    #[command(flatten)]
    pub base: EngineCreateArgs,

    /// Storage mode (local, s3, …)
    #[arg(long = "storage-mode")]
    pub storage_mode: Option<String>,
}

// `fork <pid> --name` — clone an instance's data onto a new instance.
#[derive(Debug, Clone, Args)]
pub struct ForkArgs {
    /// Source instance pid
    pub pid: String,

    /// Name for the forked instance
    #[arg(long)]
    pub name: Option<String>,
}

// `resize <pid>` — change instance type and/or storage size.
#[derive(Debug, Clone, Args)]
pub struct ResizeArgs {
    /// Instance pid
    pub pid: String,

    /// New instance type
    #[arg(long = "instance-type")]
    pub instance_type: Option<String>,

    /// New storage size, e.g. `200gb`
    #[arg(long)]
    pub size: Option<String>,
}

// `scale <pid>` — change the replica count of a group.
#[derive(Debug, Clone, Args)]
pub struct ScaleArgs {
    /// Instance or group pid
    pub pid: String,

    /// Target replica count
    #[arg(long)]
    pub replicas: Option<u32>,
}

// `failover <group>` — promote another node of a HA group.
#[derive(Debug, Clone, Args)]
pub struct FailoverArgs {
    /// Instance group id
    pub group: String,
}

// `update <pid>` — mutable instance attributes.
#[derive(Debug, Clone, Args)]
pub struct UpdateArgs {
    /// Instance pid
    pub pid: String,

    /// Tag to apply (repeatable)
    #[arg(long)]
    pub tags: Vec<String>,

    /// Expose or hide the public endpoint
    #[arg(long)]
    pub public: bool,

    /// Replace the allowed CIDR list (repeatable)
    #[arg(long = "allowed-cidr")]
    pub allowed_cidrs: Vec<String>,

    /// Refuse deletion while set
    #[arg(long = "delete-protection")]
    pub delete_protection: bool,
}

// `wait <pid>` — poll until the instance reaches a terminal state.
#[derive(Debug, Clone, Args)]
pub struct WaitArgs {
    /// Instance pid
    pub pid: String,

    /// Seconds between progress checks
    #[arg(long)]
    pub interval: Option<u64>,

    /// Stop waiting after this many seconds
    #[arg(long)]
    pub timeout: Option<u64>,
}

// `users create <pid>`.
#[derive(Clone, Args)]
pub struct UserCreateArgs {
    /// Instance pid
    pub pid: String,

    /// Name for the new user
    #[arg(long)]
    pub name: String,

    /// Password (generated when omitted). Prefer SELFHOSTDEV_DB_PASSWORD; flag values stay visible in shell history and the process list.
    #[arg(long, env = "SELFHOSTDEV_DB_PASSWORD", hide_env_values = true)]
    pub password: Option<String>,
}

impl std::fmt::Debug for UserCreateArgs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UserCreateArgs")
            .field("pid", &self.pid)
            .field("name", &self.name)
            .field("password", &self.password.as_ref().map(|_| "[REDACTED]"))
            .finish()
    }
}

// `users update <pid>`.
#[derive(Clone, Args)]
pub struct UserUpdateArgs {
    /// Instance pid
    pub pid: String,

    /// Role to update
    #[arg(long)]
    pub name: Option<String>,

    /// New password. Prefer SELFHOSTDEV_DB_PASSWORD; flag values stay visible in shell history and the process list.
    #[arg(long, env = "SELFHOSTDEV_DB_PASSWORD", hide_env_values = true)]
    pub password: Option<String>,

    /// New role (e.g. readonly)
    #[arg(long)]
    pub role: Option<String>,
}

impl std::fmt::Debug for UserUpdateArgs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UserUpdateArgs")
            .field("pid", &self.pid)
            .field("name", &self.name)
            .field("password", &self.password.as_ref().map(|_| "[REDACTED]"))
            .field("role", &self.role)
            .finish()
    }
}

#[cfg(test)]
mod organization_pid_tests {
    use super::*;
    use serde_json::json;

    /// A pid is interpolated into request paths, so only the `org_<hex>` shape
    /// is accepted: a hostile reference never becomes an organization pid.
    #[test]
    fn organization_pids_reject_hostile_or_overlong_values() {
        for hostile in [
            "../../etc/passwd",
            "org_a1/../b2",
            "org_a1/activity_logs",
            "org_a1?page=1",
            "org_a1#frag",
            "org_a1 slug",
            "org_",
        ] {
            assert!(!is_org_pid(hostile), "{hostile:?} must not be a pid");
        }

        // 64 characters is the ceiling; the 65th is refused.
        let at_bound = format!("org_{}", "a".repeat(60));
        assert_eq!(at_bound.len(), 64);
        assert!(is_org_pid(&at_bound), "a 64-character pid is accepted");
        let overlong = format!("org_{}", "a".repeat(61));
        assert_eq!(overlong.len(), 65);
        assert!(!is_org_pid(&overlong), "a 65-character pid is refused");

        // A record carrying a hostile pid counts as no pid at all.
        assert_eq!(organization_pid(&json!({"pid": "org_a1/../b2"})), None);
        assert_eq!(organization_pid(&json!({"id": overlong})), None);
        assert_eq!(organization_pid(&json!({"pid": ""})), None);
        assert_eq!(organization_pid(&json!({"pid": "org_a1"})), Some("org_a1"));
    }
}

#[cfg(test)]
mod confirmation_tests {
    use super::*;

    /// The typed confirmation exists to make a destructive verb deliberate. If
    /// the phrase it compares against is empty, an EOF or a bare Enter reads as
    /// an empty line and would confirm by accident, so an empty or
    /// whitespace-only phrase never matches.
    #[tokio::test]
    async fn an_empty_or_whitespace_confirmation_phrase_never_confirms() {
        assert!(!confirm_typed("type  to confirm: ", "").await);
        assert!(!confirm_typed("type  to confirm: ", "   ").await);
        assert!(!confirm_typed("type  to confirm: ", "\t\n").await);
    }
}

#[cfg(test)]
mod user_secret_tests {
    use super::*;

    #[derive(Parser)]
    struct CreateProbe {
        #[command(flatten)]
        args: UserCreateArgs,
    }

    #[test]
    fn debug_output_hides_database_password() {
        let create = UserCreateArgs {
            pid: "pg-1".to_string(),
            name: "app".to_string(),
            password: Some("canary-db-password-3f4a5b".to_string()),
        };
        let shown = format!("{create:?}");
        assert!(!shown.contains("canary-db-password-3f4a5b"));
        assert!(shown.contains("pg-1"));

        let update = UserUpdateArgs {
            pid: "pg-1".to_string(),
            name: Some("app".to_string()),
            password: Some("canary-db-password-6c7d8e".to_string()),
            role: None,
        };
        let shown = format!("{update:?}");
        assert!(!shown.contains("canary-db-password-6c7d8e"));
    }

    #[test]
    fn database_password_env_fallback_parses() {
        unsafe { std::env::set_var("SELFHOSTDEV_DB_PASSWORD", "env-canary-password") };
        let probe = CreateProbe::try_parse_from(["probe", "pg-1", "--name", "app"])
            .expect("env fallback must parse");
        assert_eq!(probe.args.password.as_deref(), Some("env-canary-password"));
        unsafe { std::env::remove_var("SELFHOSTDEV_DB_PASSWORD") };
    }
}

// `users delete|rotate-password` — an existing role.
#[derive(Debug, Clone, Args)]
pub struct UserRefArgs {
    /// Instance pid
    pub pid: String,

    /// User to change
    pub name: String,
}

// `config set <pid> <key> <value>`.
#[derive(Debug, Clone, Args)]
pub struct EngineConfigSetArgs {
    /// Instance pid
    pub pid: String,

    /// Parameter name, e.g. max_connections
    pub key: String,

    /// New value
    pub value: String,
}

// PITR verbs: everything except `restore`/`configure` is pid-only.
#[derive(Debug, Clone, Args)]
pub struct PitrArgs {
    /// Instance pid
    pub pid: String,

    /// How many days of recovery to keep (configure)
    #[arg(long = "retention-days")]
    pub retention_days: Option<u32>,

    /// When to take base backups (configure)
    #[arg(long)]
    pub schedule: Option<String>,

    /// Moment to restore to (restore)
    #[arg(long = "to-time")]
    pub to_time: Option<String>,
}

// `pool update <pid>`.
#[derive(Debug, Clone, Args)]
pub struct PoolUpdateArgs {
    /// Instance pid
    pub pid: String,

    /// Pool size (PgBouncer default_pool_size or ProxySQL threads)
    #[arg(long)]
    pub size: Option<u32>,

    /// Pool mode where the pooler supports one
    #[arg(long = "pool-mode")]
    pub pool_mode: Option<String>,
}

// `extensions enable <extension>`.
#[derive(Debug, Clone, Args)]
pub struct ExtensionEnableArgs {
    /// Extension name, e.g. pgvector
    pub extension: String,

    /// Instance pid
    #[arg(long)]
    pub instance: Option<String>,
}

// `durability set <pid> --mode`.
#[derive(Debug, Clone, Args)]
pub struct DurabilitySetArgs {
    /// Instance pid
    pub pid: String,

    /// Durability mode
    #[arg(long)]
    pub mode: String,
}

// `replicas create <pid>`.
#[derive(Debug, Clone, Args)]
pub struct ReplicaCreateArgs {
    /// Instance pid
    pub pid: String,

    /// Replica name
    #[arg(long)]
    pub name: Option<String>,

    /// Number of replicas to add
    #[arg(long)]
    pub count: Option<u32>,
}

// `replicas delete <pid> <replica>`.
#[derive(Debug, Clone, Args)]
pub struct ReplicaDeleteArgs {
    /// Instance pid
    pub pid: String,

    /// Replica to remove
    pub replica: String,
}

// Top-level command families (design §3, in documented order).
#[derive(Debug, Clone, Subcommand)]
pub enum Command {
    /// Sign in, sign out and inspect the current session
    #[command(subcommand)]
    Auth(auth::AuthCommand),

    /// Manage saved profiles: API and console endpoints, default org
    #[command(subcommand)]
    Profile(profile::ProfileCommand),

    /// Read and write your saved default settings
    #[command(subcommand)]
    Config(config::ConfigCommand),

    /// Organizations, members, invitations and activity
    #[command(subcommand)]
    Org(org::OrgCommand),

    /// Projects: their databases, services, backups, snapshots and SSH
    #[command(subcommand)]
    Project(project::ProjectCommand),

    /// Deploy a GitHub repository and manage runs, env vars and domains
    #[command(subcommand)]
    Deploy(deploy::DeployCommand),

    /// Connect GitHub and inspect repository branches and build settings
    #[command(subcommand)]
    Github(github::GithubCommand),

    /// Custom domains for your organization and their DNS status
    #[command(subcommand)]
    Domain(domain::DomainCommand),

    /// Managed PostgreSQL databases
    #[command(subcommand)]
    Postgres(postgres::PostgresCommand),

    /// Managed MySQL databases
    #[command(subcommand)]
    Mysql(mysql::MysqlCommand),

    /// Managed MongoDB databases
    #[command(subcommand)]
    Mongo(mongo::MongoCommand),

    /// Managed Redis databases
    #[command(subcommand)]
    Redis(redis::RedisCommand),

    /// Managed ClickHouse databases
    #[command(subcommand)]
    Clickhouse(clickhouse::ClickhouseCommand),

    /// Managed OpenSearch databases
    #[command(subcommand)]
    Opensearch(opensearch::OpensearchCommand),

    /// Regions, instance types, storage types and cost estimates
    #[command(subcommand)]
    Catalog(catalog::CatalogCommand),

    /// Wallet, top-ups, transactions and auto-recharge
    #[command(subcommand)]
    Billing(billing::BillingCommand),

    /// Cloud provider credentials and the default provider
    #[command(subcommand)]
    Cloud(cloud::CloudCommand),

    /// VPCs, subnets and security groups
    #[command(subcommand)]
    Network(network::NetworkCommand),

    /// SSH keys for your organization and projects
    #[command(name = "ssh-key")]
    #[command(subcommand)]
    SshKey(ssh_key::SshKeyCommand),

    /// Alert rules, fired alerts and notification channels
    #[command(subcommand)]
    Alert(alert::AlertCommand),

    /// Scaling policies, capacity ladders and scale plans
    #[command(subcommand)]
    Scaling(scaling::ScalingCommand),

    /// Webhook endpoints for your organization
    #[command(subcommand)]
    Webhook(webhook::WebhookCommand),

    /// Call any platform endpoint directly
    Api(api::ApiArgs),

    /// Move this CLI to the newest published build
    Update(update::UpdateArgs),

    /// Open the interactive terminal UI
    Tui(tui::TuiArgs),

    /// Show help for a command
    Help(help::HelpArgs),

    /// Print every command in the CLI as a tree
    Tree(NoArgs),

    /// Generate shell completions
    Completion(completion::CompletionArgs),
}

/// The subject the gate refuses `--dry-run` with, derived from the command it
/// was handed. A run that never reaches the gate — the bare invocation — reads
/// the same vocabulary from [`dry_run_subject_of_group`].
fn dry_run_subject(command: &Command) -> &'static str {
    match command {
        Command::Api(_) => dry_run_subject_of_group(Some("api")),
        Command::Org(_) => dry_run_subject_of_group(Some("org")),
        _ => dry_run_subject_of_group(None),
    }
}

// Root parser: global options plus the command family (design §3).
//
// The help trailer is not an attribute: `after_help` is generated from the
// registered command tree by [`command`], so the "every engine group supports"
// line can never drift from the verbs the engine groups actually register.
#[derive(Debug, Parser)]
#[command(
    name = "selfhost",
    bin_name = "selfhost",
    version,
    about = "selfhost — the SelfHost platform CLI",
    long_about = "selfhost — the SelfHost platform CLI\n\nManage servers, managed databases, projects and deployments on selfhost.dev.",
    override_usage = "selfhost [OPTIONS] <COMMAND> [ARGS]",
    disable_help_subcommand = true,
    disable_help_flag = true,
    disable_version_flag = true,
    help_template = "{before-help}{about-with-newline}\n{usage-heading} {usage}\n\n{all-args}{after-help}"
)]
pub struct Cli {
    #[command(flatten)]
    pub global: GlobalArgs,

    #[command(subcommand)]
    pub command: Command,
}

// Engine groups whose shared surface the `--help` trailer summarises.
const ENGINE_GROUPS: [&str; 6] = [
    "postgres",
    "mysql",
    "mongo",
    "redis",
    "clickhouse",
    "opensearch",
];

// Root command as the binary uses it: the derive-generated tree plus the
// generated trailer.
pub fn command() -> clap::Command {
    let command = Cli::command();
    let trailer = help_trailer(&command);
    command.after_help(trailer)
}

// Verbs every engine group registers, in the order `postgres` declares them.
//
// Computed from the tree rather than hand-written: the trailer is true by
// construction, and pruning an engine verb updates it automatically.
fn shared_engine_verbs(command: &clap::Command) -> Vec<String> {
    let mut groups = ENGINE_GROUPS.iter().filter_map(|name| {
        command
            .get_subcommands()
            .find(|cmd| cmd.get_name() == *name)
    });
    let Some(first) = groups.next() else {
        return Vec::new();
    };
    let mut shared: Vec<&str> = first
        .get_subcommands()
        .map(clap::Command::get_name)
        .collect();
    for group in groups {
        let names: Vec<&str> = group
            .get_subcommands()
            .map(clap::Command::get_name)
            .collect();
        shared.retain(|verb| names.contains(verb));
    }
    shared.into_iter().map(str::to_owned).collect()
}

// Trailer under `selfhost --help`: what the engine groups have in common, where
// `--provider` lives, and worked examples.
fn help_trailer(command: &clap::Command) -> String {
    let mut trailer = String::from(
        "Managed databases:\n  \
         Each engine group (postgres, mysql, mongo, redis, clickhouse, opensearch)\n  \
         manages one engine. Pass --provider aws|hetzner to create and list to\n  \
         choose the cloud it runs on (default: your profile's provider).\n\n\
         Every engine group supports:\n",
    );
    // Wrapped here rather than by clap, so the verb list breaks on a comma.
    let mut line = String::from("  ");
    for verb in shared_engine_verbs(command) {
        let candidate = format!("{line}{verb},");
        if candidate.len() > 74 {
            trailer.push_str(line.trim_end_matches(' '));
            trailer.push('\n');
            line = String::from("  ");
        }
        line.push_str(&format!("{verb}, "));
    }
    trailer.push_str(line.trim_end_matches(", "));
    trailer.push_str(
        "\n\nExamples:\n  \
         selfhost auth login                              # sign in to the default profile\n  \
         selfhost --profile qa postgres list --format json # list QA PostgreSQL databases\n  \
         selfhost postgres create --provider hetzner --name pg-staging --ha\n  \
         selfhost project db create postgres --project acme-api --name app-db\n  \
         selfhost deploy trigger acme-api --branch main --follow\n  \
         selfhost tui   # open the interactive terminal UI\n\n\
         Run selfhost with no arguments to open the interactive terminal UI\n  \
         (same as selfhost tui). Set SELFHOSTDEV_NO_TUI=1 to print help instead.",
    );
    trailer
}

impl Cli {
    /// Dispatch the parsed command tree.
    ///
    /// `auth`, `profile` and `org` reach their own dispatch; the remaining
    /// families return the staging error for their full command path until
    /// their slice implements them.
    pub fn run(self) -> crate::error::Result<()> {
        let Cli { global, command } = self;

        // The dry-run gate owns every refusal: no command simulates its
        // request yet, so with the flag set nothing may be sent, written, or
        // read in order to change something. It runs here, in the shared
        // dispatch, before any handler — a handler must never send under the
        // flag, and a command that really previews its request later has to be
        // exempted here on purpose, never by dropping a check of its own.
        if global.dry_run {
            return reject_dry_run_for(&global, dry_run_subject(&command));
        }

        match command {
            Command::Api(args) => block_on(api::run(&global, args)),
            Command::Tui(args) => tui::run(args),
            Command::Help(args) => help::run(args),
            Command::Tree(_) => tree::run(),
            Command::Update(args) => block_on(update::run(&global, args)),
            Command::Completion(args) => completion::run(args),

            Command::Auth(command) => command.dispatch(&global),
            Command::Profile(command) => command.dispatch(&global),
            Command::Config(command) => command.dispatch(),
            Command::Org(command) => command.dispatch(&global),
            Command::Project(command) => command.dispatch(),
            Command::Deploy(command) => command.dispatch(),
            Command::Github(command) => command.dispatch(),
            Command::Domain(command) => command.dispatch(),
            Command::Postgres(command) => command.dispatch(),
            Command::Mysql(command) => command.dispatch(),
            Command::Mongo(command) => command.dispatch(),
            Command::Redis(command) => command.dispatch(),
            Command::Clickhouse(command) => command.dispatch(),
            Command::Opensearch(command) => command.dispatch(),
            Command::Catalog(command) => command.dispatch(),
            Command::Billing(command) => command.dispatch(),
            Command::Cloud(command) => command.dispatch(),
            Command::Network(command) => command.dispatch(),
            Command::SshKey(command) => command.dispatch(),
            Command::Alert(command) => command.dispatch(),
            Command::Scaling(command) => command.dispatch(),
            Command::Webhook(command) => command.dispatch(),
        }
    }
}
