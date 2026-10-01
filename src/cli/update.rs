//! `update` — move this CLI to the newest published build.
//!
//! All of the work lives in [`crate::update`]; this file is only the command
//! line. The one thing it does that the module cannot is name the file to
//! replace: `std::env::current_exe()`. Passing it in is what makes the whole
//! install path testable against a temporary file instead of a real binary.

use std::path::PathBuf;
use std::time::Duration;

use clap::Args;
use semver::Version;
use serde_json::{Value, json};

use crate::error::{Error, Result};
use crate::update::{self, Config, Outcome};

use super::{GlobalArgs, human_output, print};

// Arguments for `selfhost update`.
#[derive(Debug, Clone, Args)]
pub struct UpdateArgs {
    /// Report the installed and the newest version, install nothing
    #[arg(long)]
    pub check: bool,

    /// Where to look for the newest release
    #[arg(long, value_name = "URL")]
    pub manifest_url: Option<String>,
}

/// `selfhost update`: resolve where to look, name the file to replace, and
/// report what happened.
pub async fn run(global: &GlobalArgs, args: UpdateArgs) -> Result<()> {
    let mut config = Config::new(args.check, Duration::from_secs(global.timeout));
    config.manifest_url = update::resolve_manifest_url(
        args.manifest_url.as_deref(),
        std::env::var(update::MANIFEST_URL_ENV).ok().as_deref(),
    );
    let current = config.local.clone();
    let target = current_exe()?;

    let outcome = update::run(&config, &target).await?;
    if human_output(global) {
        println!("{}", human_line(&current, &outcome));
    } else {
        print(global, &report(&current, &outcome))?;
    }
    Ok(())
}

/// The running executable. A process that cannot name its own binary has
/// nothing safe to replace.
fn current_exe() -> Result<PathBuf> {
    std::env::current_exe().map_err(|err| {
        Error::Other(
            anyhow::Error::from(err).context("cannot find the running selfhost executable"),
        )
    })
}

/// The single line a person reads.
fn human_line(current: &Version, outcome: &Outcome) -> String {
    match outcome {
        Outcome::UpToDate { latest } => {
            format!("selfhost {latest} is already the newest release")
        }
        Outcome::LocalIsNewer { latest } => {
            format!("selfhost {current} is newer than the newest release, {latest}")
        }
        Outcome::PreRelease { latest } => format!(
            "selfhost {current} is a pre-release; updates only move to stable releases, and the newest is {latest}"
        ),
        Outcome::Checked {
            latest, available, ..
        } => format!(
            "selfhost {current} is installed, {latest} is the newest release, update available: {}",
            if *available { "yes" } else { "no" }
        ),
        Outcome::Installed {
            version, backup, ..
        } => match backup {
            // A running Windows build cannot be deleted, so the one it
            // replaced stays on disk and the caller has to be told where.
            Some(previous) => format!(
                "updated to selfhost {version}; the build it replaced is at {}",
                previous.display()
            ),
            None => format!("updated to selfhost {version}"),
        },
    }
}

/// The machine-readable form, in the resolved output format.
fn report(current: &Version, outcome: &Outcome) -> Value {
    match outcome {
        Outcome::UpToDate { latest } | Outcome::LocalIsNewer { latest } => json!({
            "current": current.to_string(),
            "latest": latest.to_string(),
            "updated": false,
        }),
        Outcome::PreRelease { latest } => json!({
            "current": current.to_string(),
            "latest": latest.to_string(),
            "prerelease": true,
            "updated": false,
        }),
        Outcome::Checked {
            latest, available, ..
        } => json!({
            "current": current.to_string(),
            "latest": latest.to_string(),
            "update_available": available,
            "updated": false,
        }),
        Outcome::Installed {
            version,
            target,
            backup,
        } => {
            let mut report = json!({
                "current": version.to_string(),
                "previous": current.to_string(),
                "updated": true,
                "path": target.display().to_string(),
            });
            if let Some(previous) = backup {
                report["previous_path"] = Value::from(previous.display().to_string());
            }
            report
        }
    }
}
