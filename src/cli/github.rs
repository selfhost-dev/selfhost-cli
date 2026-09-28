//! `github` — installations, repo branches, build-config detection (design §4).

use clap::Args;

use super::*;

// Repository selector shared by the `github` leaves.
#[derive(Debug, Clone, Args)]
pub struct GithubRepoArgs {
    /// Repository (`owner/name`)
    #[arg(long)]
    pub repo: Option<String>,
}

stub_group!(
    /// `github installations` — GitHub App installs.
    GithubInstallationsCommand, "github installations",
    leaves {
        List(NoArgs) => "list",
    }
    groups {}
);

stub_group!(
    /// `github branches` — branches of a repo.
    GithubBranchesCommand, "github branches",
    leaves {
        List(GithubRepoArgs) => "list",
    }
    groups {}
);

stub_group!(
    /// GitHub installations, repo branches, build-config detection.
    GithubCommand, "github",
    leaves {
        Detect(GithubRepoArgs) => "detect",
        Connect(GithubRepoArgs) => "connect",
        Disconnect(GithubRepoArgs) => "disconnect",
    }
    groups {
        Installations(GithubInstallationsCommand) => "installations",
        Branches(GithubBranchesCommand) => "branches",
    }
);
