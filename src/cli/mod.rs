//! Command tree and dispatch (design §3, §4, §7).
//!
//! Slice 0 registers the complete documented surface as real clap subcommands and
//! stages the behaviour: commands whose implementation a later slice owns answer with
//! `not implemented yet: <full command path>`. Only `help`, `tree` and `completion`
//! do work here.
//!
//! * adding a command: one line in the group's [`stub_group!`] call;
//! * adding behaviour: a real arm in the group's dispatch, or a new group module.

use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::output::Format;

pub mod alert;
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

// Built-in `--env` shorthands, equivalent to the profiles of the same name (§5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum EnvKind {
    /// Production (`api.selfhost.dev`).
    Prod,
    /// QA (`qapi.selfhost.dev`).
    Qa,
    /// Local Rails console (`http://localhost:3000`).
    Local,
}

// Cloud a database instance is provisioned on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Provider {
    /// AWS.
    Aws,
    /// Hetzner Cloud.
    Hetzner,
}

// Options accepted by every command, before or after the subcommand (design §3).
#[derive(Debug, Clone, Args)]
pub struct GlobalArgs {
    /// Profile to use [default: "default"]
    #[arg(
        short = 'p',
        long,
        global = true,
        env = "SELFHOST_PROFILE",
        value_name = "NAME",
        help_heading = "Global options"
    )]
    pub profile: Option<String>,

    /// API base URL [default: the profile's, else https://api.selfhost.dev]
    #[arg(
        long = "base-url",
        global = true,
        env = "SELFHOST_BASE_URL",
        value_name = "URL",
        help_heading = "Global options"
    )]
    pub base_url: Option<String>,

    /// Shorthand for a built-in profile
    #[arg(
        long,
        global = true,
        value_enum,
        value_name = "ENV",
        help_heading = "Global options"
    )]
    pub env: Option<EnvKind>,

    /// Organization (slug or pid) [default: the profile's org]
    #[arg(
        long,
        global = true,
        env = "SELFHOST_ORG",
        value_name = "SLUG",
        help_heading = "Global options"
    )]
    pub org: Option<String>,

    /// Output format [default: table on TTY, json when piped]
    #[arg(
        short = 'o',
        long = "format",
        global = true,
        value_enum,
        value_name = "FMT",
        help_heading = "Global options"
    )]
    pub format: Option<Format>,

    /// Shorthand for --format json
    #[arg(long, global = true, help_heading = "Global options")]
    pub json: bool,

    /// Disable ANSI output
    #[arg(long = "no-color", global = true, help_heading = "Global options")]
    pub no_color: bool,

    /// Overall request timeout
    #[arg(
        long,
        global = true,
        default_value_t = 30,
        value_name = "SECS",
        help_heading = "Global options"
    )]
    pub timeout: u64,

    /// Override the per-operation poll cadence
    #[arg(
        long = "poll-interval",
        global = true,
        value_name = "SECS",
        help_heading = "Global options"
    )]
    pub poll_interval: Option<u64>,

    /// Assume yes; skips destructive confirmations
    #[arg(short = 'y', long, global = true, help_heading = "Global options")]
    pub yes: bool,

    /// Print the HTTP request without sending it
    #[arg(long = "dry-run", global = true, help_heading = "Global options")]
    pub dry_run: bool,

    /// Errors only
    #[arg(short = 'q', long, global = true, help_heading = "Global options")]
    pub quiet: bool,

    /// Show HTTP requests
    #[arg(short = 'v', long, global = true, help_heading = "Global options")]
    pub verbose: bool,

    /// Show request/response bodies (secrets redacted)
    #[arg(long, global = true, help_heading = "Global options")]
    pub debug: bool,
}

// A command with no documented flags yet.
#[derive(Debug, Clone, Args)]
pub struct NoArgs {}

// A single required positional: pid (`awsinst_*`), group id, or resource name.
#[derive(Debug, Clone, Args)]
pub struct TargetArgs {
    /// Instance pid, group id or resource name
    pub target: String,
}

// One optional positional — listings that can be scoped or left org-wide.
#[derive(Debug, Clone, Args)]
pub struct OptionalTargetArgs {
    /// Instance pid, group id or resource name
    pub target: Option<String>,
}

// A required instance pid, for the per-instance sub-resources.
#[derive(Debug, Clone, Args)]
pub struct PidArgs {
    /// Instance pid (`awsinst_*`)
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

    /// Log source (engine-specific, e.g. `postgres`, `pgbouncer`)
    #[arg(long)]
    pub source: Option<String>,

    /// Keep polling until the run/instance ends (honors 429 cooldowns)
    #[arg(long)]
    pub follow: bool,

    /// Number of tail lines
    #[arg(long)]
    pub lines: Option<u32>,

    /// Start time (RFC 3339 or relative like `1h`)
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

    /// Public key body (`ssh-ed25519 AAAA…`) or a path to a `.pub` file
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

    /// Access mode (`all`, `bastion`, `read-only`, …)
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

    /// Engine version, e.g. `16`
    #[arg(long)]
    pub version: Option<String>,

    /// Region, e.g. `us-east-1` / `fsn1`
    #[arg(long)]
    pub region: Option<String>,

    /// Instance type from `selfhost catalog instance-types`
    #[arg(long = "instance-type")]
    pub instance_type: Option<String>,

    /// Storage type from `selfhost catalog storage-types`
    #[arg(long = "storage-type")]
    pub storage_type: Option<String>,

    /// Storage size, e.g. `100gb`
    #[arg(long)]
    pub size: Option<String>,

    /// Cloud to provision on
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

    /// Storage mode (`local`, `s3`, …)
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

    /// Poll cadence in seconds (never below the API cooldowns)
    #[arg(long)]
    pub interval: Option<u64>,

    /// Give up after this many seconds
    #[arg(long)]
    pub timeout: Option<u64>,
}

// `users create <pid>`.
#[derive(Debug, Clone, Args)]
pub struct UserCreateArgs {
    /// Instance pid
    pub pid: String,

    /// Role name to create
    #[arg(long)]
    pub name: String,

    /// Password (generated when omitted)
    #[arg(long)]
    pub password: Option<String>,
}

// `users update <pid>`.
#[derive(Debug, Clone, Args)]
pub struct UserUpdateArgs {
    /// Instance pid
    pub pid: String,

    /// Role to update
    #[arg(long)]
    pub name: Option<String>,

    /// New password
    #[arg(long)]
    pub password: Option<String>,

    /// New role attributes (e.g. `readonly`)
    #[arg(long)]
    pub role: Option<String>,
}

// `users delete|rotate-password` — an existing role.
#[derive(Debug, Clone, Args)]
pub struct UserRefArgs {
    /// Instance pid
    pub pid: String,

    /// Existing role name
    pub name: String,
}

// `config set <pid> <key> <value>`.
#[derive(Debug, Clone, Args)]
pub struct EngineConfigSetArgs {
    /// Instance pid
    pub pid: String,

    /// Parameter name, e.g. `max_connections`
    pub key: String,

    /// New value
    pub value: String,
}

// PITR verbs: everything except `restore`/`configure` is pid-only.
#[derive(Debug, Clone, Args)]
pub struct PitrArgs {
    /// Instance pid
    pub pid: String,

    /// Retention window in days (`configure`)
    #[arg(long = "retention-days")]
    pub retention_days: Option<u32>,

    /// Backup schedule window (`configure`)
    #[arg(long)]
    pub schedule: Option<String>,

    /// Point in time to restore to (`restore`)
    #[arg(long = "to-time")]
    pub to_time: Option<String>,
}

// `pool update <pid>`.
#[derive(Debug, Clone, Args)]
pub struct PoolUpdateArgs {
    /// Instance pid
    pub pid: String,

    /// Pool size (pgbouncer `default_pool_size` / proxysql threads)
    #[arg(long)]
    pub size: Option<u32>,

    /// Pool mode where the pooler supports one
    #[arg(long = "pool-mode")]
    pub pool_mode: Option<String>,
}

// `extensions enable <extension>`.
#[derive(Debug, Clone, Args)]
pub struct ExtensionEnableArgs {
    /// Extension name, e.g. `pgvector`
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

    /// Replica identifier
    pub replica: String,
}

// Top-level command families (design §3, in documented order).
#[derive(Debug, Clone, Subcommand)]
pub enum Command {
    /// Sign in/out, inspect the session (browser OAuth, per profile)
    #[command(subcommand)]
    Auth(auth::AuthCommand),

    /// Manage profiles — name, API base URL, console URL
    #[command(subcommand)]
    Profile(profile::ProfileCommand),

    /// Read/write defaults (org, provider, format, timeout)
    #[command(subcommand)]
    Config(config::ConfigCommand),

    /// Organizations, members, invitations, activity log
    #[command(subcommand)]
    Org(org::OrgCommand),

    /// Projects: in-project databases, services, backups, snapshots, SSH
    #[command(subcommand)]
    Project(project::ProjectCommand),

    /// GitHub repo deployments: deploy, runs, logs, env vars, domains
    #[command(subcommand)]
    Deploy(deploy::DeployCommand),

    /// GitHub installations, repo branches, build-config detection
    #[command(subcommand)]
    Github(github::GithubCommand),

    /// Org-level custom domains and DNS verification
    #[command(subcommand)]
    Domain(domain::DomainCommand),

    /// Managed PostgreSQL clusters (--provider aws|hetzner)
    #[command(subcommand)]
    Postgres(postgres::PostgresCommand),

    /// Managed MySQL clusters
    #[command(subcommand)]
    Mysql(mysql::MysqlCommand),

    /// Managed MongoDB clusters
    #[command(subcommand)]
    Mongo(mongo::MongoCommand),

    /// Managed Redis clusters
    #[command(subcommand)]
    Redis(redis::RedisCommand),

    /// Managed ClickHouse clusters
    #[command(subcommand)]
    Clickhouse(clickhouse::ClickhouseCommand),

    /// Managed OpenSearch clusters
    #[command(subcommand)]
    Opensearch(opensearch::OpensearchCommand),

    /// Regions, instance types, storage types, cost estimates
    #[command(subcommand)]
    Catalog(catalog::CatalogCommand),

    /// Wallet, top-ups, transactions, SKUs, auto-recharge
    #[command(subcommand)]
    Billing(billing::BillingCommand),

    /// Cloud credentials and the default provider
    #[command(subcommand)]
    Cloud(cloud::CloudCommand),

    /// VPCs, subnets, security groups
    #[command(subcommand)]
    Network(network::NetworkCommand),

    /// Organization and project SSH keys, project SSH access
    #[command(name = "ssh-key")]
    #[command(subcommand)]
    SshKey(ssh_key::SshKeyCommand),

    /// Alert rules, triggered instances, notification channels
    #[command(subcommand)]
    Alert(alert::AlertCommand),

    /// Scaling policies; capacity ladders and scale plans
    #[command(subcommand)]
    Scaling(scaling::ScalingCommand),

    /// Organization webhook endpoints
    #[command(subcommand)]
    Webhook(webhook::WebhookCommand),

    /// Same as `<command> --help`
    Help(help::HelpArgs),

    /// Print the full command tree (kept for agent workflows)
    Tree(NoArgs),

    /// Generate shell completions (bash, zsh, fish)
    Completion(completion::CompletionArgs),
}

// Root parser: global options plus the command family (design §3).
#[derive(Debug, Parser)]
#[command(
    name = "selfhost",
    bin_name = "selfhost",
    version,
    about = "selfhost — the SelfHost platform CLI",
    long_about = "selfhost — the SelfHost platform CLI\n\nManage servers, managed databases, projects and deployments on selfhost.dev.",
    override_usage = "selfhost [OPTIONS] <COMMAND> [ARGS]",
    disable_help_subcommand = true,
    help_template = "{before-help}{about-with-newline}\n{usage-heading} {usage}\n\n{all-args}{after-help}",
    after_help = "Every engine group shares: list, show, create, delete, start, stop, reboot,\nfork, resize, scale, failover, update, wait, logs, stats, metrics, users,\nsnapshots, backups, pitr, pool, config.\n\nExamples:\n  selfhost auth login                         # browser OAuth for the default profile\n  selfhost --env qa postgres list --format json\n  selfhost postgres create --provider hetzner --name pg-staging --ha\n  selfhost project db create postgres --project acme-api --name app-db\n  selfhost deploy trigger acme-api --branch main --follow"
)]
pub struct Cli {
    #[command(flatten)]
    pub global: GlobalArgs,

    #[command(subcommand)]
    pub command: Command,
}

impl Cli {
    /// Dispatch the parsed command tree.
    ///
    /// Slice 0: only `help`, `tree` and `completion` reach an implementation; every
    /// other family returns the staging error for its full command path.
    pub fn run(self) -> crate::error::Result<()> {
        match self.command {
            Command::Help(args) => help::run(args),
            Command::Tree(_) => tree::run(),
            Command::Completion(args) => completion::run(args),

            Command::Auth(command) => command.dispatch(),
            Command::Profile(command) => command.dispatch(),
            Command::Config(command) => command.dispatch(),
            Command::Org(command) => command.dispatch(),
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
