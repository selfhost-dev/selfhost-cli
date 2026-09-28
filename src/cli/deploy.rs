//! `deploy` — GitHub repo deployments: deploy, runs, logs, env vars, domains
//! (design §4).

use clap::Args;

use super::*;

// `deploy create` — connect a repo (the GitHub half lives in `github`).
#[derive(Debug, Clone, Args)]
pub struct DeployCreateArgs {
    /// Project the deployment belongs to
    #[arg(long)]
    pub project: Option<String>,

    /// Repository to deploy (owner/name)
    #[arg(long)]
    pub repo: Option<String>,

    /// Branch to track
    #[arg(long)]
    pub branch: Option<String>,

    /// Name for the deployment
    #[arg(long)]
    pub name: Option<String>,
}

// `deploy trigger` — start a run (optionally streaming its build log).
#[derive(Debug, Clone, Args)]
pub struct DeployTriggerArgs {
    /// Deployment to deploy
    pub target: Option<String>,

    /// Branch to build
    #[arg(long)]
    pub branch: Option<String>,

    /// Keep streaming the build log until the run ends
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
    /// Builds of a deployment
    DeployRunsCommand, "deploy runs",
    leaves {
        /// List runs
        List(OptionalTargetArgs) => "list",
        /// Show a run's build log
        Logs(LogsArgs) => "logs",
        /// Deploy the current commit again
        Redeploy(TargetArgs) => "redeploy",
        /// Roll back to an earlier run
        Rollback(TargetArgs) => "rollback",
    }
    groups {}
);

stub_group!(
    /// Environment variables
    DeployEnvCommand, "deploy env",
    leaves {
        /// List variables
        List(OptionalTargetArgs) => "list",
        /// Set a variable
        Set(EnvSetArgs) => "set",
        /// Add several variables at once
        Merge(EnvSetArgs) => "merge",
        /// Delete a variable
        Unset(EnvUnsetArgs) => "unset",
        /// Detect variables from the repository
        Detect(OptionalTargetArgs) => "detect",
    }
    groups {}
);

stub_group!(
    /// Domains served by a deployment
    DeployDomainCommand, "deploy domain",
    leaves {
        /// List domains
        List(OptionalTargetArgs) => "list",
        /// Add a domain
        Add(DomainRefArgs) => "add",
        /// Remove a domain
        Remove(DomainRefArgs) => "remove",
        /// Check a domain's DNS
        Verify(DomainRefArgs) => "verify",
        /// Re-sync domains with the platform
        Sync(OptionalTargetArgs) => "sync",
    }
    groups {}
);

stub_group!(
    /// Who gets told about deploys
    DeployNotifyCommand, "deploy notify",
    leaves {
        /// List notification bindings
        List(OptionalTargetArgs) => "list",
        /// Notify a channel about an event
        Bind(NotifyBindArgs) => "bind",
        /// Stop notifying a channel
        Unbind(NotifyBindArgs) => "unbind",
    }
    groups {}
);

stub_group!(
    /// How the repository is built
    DeployBuildConfigCommand, "deploy build-config",
    leaves {
        /// Show the build settings
        Get(OptionalTargetArgs) => "get",
        /// Change the build settings
        Set(OptionalTargetArgs) => "set",
    }
    groups {}
);

stub_group!(
    /// Deploy a GitHub repository and manage runs, env vars and domains
    DeployCommand, "deploy",
    leaves {
        /// Connect a repository for deployment
        Create(DeployCreateArgs) => "create",
        /// List deployments
        List(NoArgs) => "list",
        /// Show one deployment
        Show(TargetArgs) => "show",
        /// Change a deployment's settings
        Update(TargetArgs) => "update",
        /// Delete a deployment
        Delete(TargetArgs) => "delete",
        /// Start a deploy now
        Trigger(DeployTriggerArgs) => "trigger",
        /// Cancel a running deploy
        Abort(TargetArgs) => "abort",
        /// Check whether a deployment is healthy
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
