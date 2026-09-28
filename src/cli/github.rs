//! `github` — installations, repo branches, build-config detection (design §4).

use clap::Args;

use super::*;

// Repository selector shared by the `github` leaves.
#[derive(Debug, Clone, Args)]
pub struct GithubRepoArgs {
    /// Repository (owner/name)
    #[arg(long)]
    pub repo: Option<String>,
}

stub_group!(
    /// GitHub accounts connected to SelfHost
    GithubInstallationsCommand, "github installations",
    leaves {
        /// List GitHub installations
        List(NoArgs) => "list",
    }
    groups {}
);

stub_group!(
    /// Branches in a repository
    GithubBranchesCommand, "github branches",
    leaves {
        /// List branches
        List(GithubRepoArgs) => "list",
    }
    groups {}
);

stub_group!(
    /// Connect GitHub and inspect repository branches and build settings
    GithubCommand, "github",
    leaves {
        /// Detect a repository's build settings
        Detect(GithubRepoArgs) => "detect",
        /// Connect a repository
        Connect(GithubRepoArgs) => "connect",
        /// Disconnect a repository
        Disconnect(GithubRepoArgs) => "disconnect",
    }
    groups {
        Installations(GithubInstallationsCommand) => "installations",
        Branches(GithubBranchesCommand) => "branches",
    }
);
