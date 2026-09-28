//! `deploy` — GitHub repo deployments: deploy, runs, logs, env vars, domains
//! (design §4).

use clap::Args;

use super::*;

// `deploy create` — connect a repo (the GitHub half lives in `github`).
#[derive(Debug, Clone, Args)]
pub struct DeployCreateArgs {
    /// Owning project
    #[arg(long)]
    pub project: Option<String>,

    /// Repository (`owner/name`)
    #[arg(long)]
    pub repo: Option<String>,

    /// Branch to track
    #[arg(long)]
    pub branch: Option<String>,

    /// Deployment name
    #[arg(long)]
    pub name: Option<String>,
}

// `deploy trigger` — start a run (optionally streaming its build log).
#[derive(Debug, Clone, Args)]
pub struct DeployTriggerArgs {
    /// Deployment/repo to trigger
    pub target: Option<String>,

    /// Branch to build
    #[arg(long)]
    pub branch: Option<String>,

    /// Follow the build log until the run ends
    #[arg(long)]
    pub follow: bool,
}

// `deploy env set|merge`.
#[derive(Debug, Clone, Args)]
pub struct EnvSetArgs {
    /// Deployment or project
    pub target: Option<String>,

    /// Variable name
    #[arg(long)]
    pub key: Option<String>,

    /// Variable value
    #[arg(long)]
    pub value: Option<String>,
}

// `deploy env unset`.
#[derive(Debug, Clone, Args)]
pub struct EnvUnsetArgs {
    /// Deployment or project
    pub target: Option<String>,

    /// Variable name
    #[arg(long)]
    pub key: Option<String>,
}

// `deploy notify bind|unbind`.
#[derive(Debug, Clone, Args)]
pub struct NotifyBindArgs {
    /// Deployment
    pub target: Option<String>,

    /// Notification channel
    #[arg(long)]
    pub channel: Option<String>,

    /// Event to notify on
    #[arg(long)]
    pub event: Option<String>,
}

stub_group!(
    /// `deploy runs` — build runs and their logs.
    DeployRunsCommand, "deploy runs",
    leaves {
        List(OptionalTargetArgs) => "list",
        Logs(LogsArgs) => "logs",
        Redeploy(TargetArgs) => "redeploy",
        Rollback(TargetArgs) => "rollback",
    }
    groups {}
);

stub_group!(
    /// `deploy env` — environment variables.
    DeployEnvCommand, "deploy env",
    leaves {
        List(OptionalTargetArgs) => "list",
        Set(EnvSetArgs) => "set",
        Merge(EnvSetArgs) => "merge",
        Unset(EnvUnsetArgs) => "unset",
        Detect(OptionalTargetArgs) => "detect",
    }
    groups {}
);

stub_group!(
    /// `deploy domain` — deployment domains.
    DeployDomainCommand, "deploy domain",
    leaves {
        List(OptionalTargetArgs) => "list",
        Add(DomainRefArgs) => "add",
        Remove(DomainRefArgs) => "remove",
        Verify(DomainRefArgs) => "verify",
        Sync(OptionalTargetArgs) => "sync",
    }
    groups {}
);

stub_group!(
    /// `deploy notify` — deployment notifications.
    DeployNotifyCommand, "deploy notify",
    leaves {
        List(OptionalTargetArgs) => "list",
        Bind(NotifyBindArgs) => "bind",
        Unbind(NotifyBindArgs) => "unbind",
    }
    groups {}
);

stub_group!(
    /// `deploy build-config` — build settings.
    DeployBuildConfigCommand, "deploy build-config",
    leaves {
        Get(OptionalTargetArgs) => "get",
        Set(OptionalTargetArgs) => "set",
    }
    groups {}
);

stub_group!(
    /// GitHub repo deployments: deploy, runs, logs, env vars, domains.
    DeployCommand, "deploy",
    leaves {
        Create(DeployCreateArgs) => "create",
        List(NoArgs) => "list",
        Show(TargetArgs) => "show",
        Update(TargetArgs) => "update",
        Delete(TargetArgs) => "delete",
        Trigger(DeployTriggerArgs) => "trigger",
        Abort(TargetArgs) => "abort",
        Health(TargetArgs) => "health",
    }
    groups {
        Runs(DeployRunsCommand) => "runs",
        Env(DeployEnvCommand) => "env",
        Domain(DeployDomainCommand) => "domain",
        Notify(DeployNotifyCommand) => "notify",
        BuildConfig(DeployBuildConfigCommand) => "build-config",
    }
);
