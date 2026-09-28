//! `profile` — named API/console endpoints (design §5).

use clap::Args;
use serde_json::{Value, json};

use crate::config::{PROD_CONSOLE_URL, Profile, ProfileStore, validate_endpoint};
use crate::error::{Error, Result};
use crate::output::Format;

use super::{GlobalArgs, NameArgs};

// `profile add <name>` — the base URL comes from global `--base-url`/`--org`.
#[derive(Debug, Clone, Args)]
pub struct ProfileAddArgs {
    /// Name for the new profile
    pub name: String,

    /// API base URL for the new profile (default: the global --base-url)
    #[arg(long = "base-url")]
    pub base_url: Option<String>,

    /// Console URL used for the browser sign-in step
    #[arg(long = "console-url")]
    pub console_url: Option<String>,
}

// `profile set <name> <key> <value>`.
#[derive(Debug, Clone, Args)]
pub struct ProfileSetArgs {
    /// Profile to change
    pub name: String,

    /// Setting to write (base_url, console_url, org, provider)
    pub key: String,

    /// New value
    pub value: String,
}

/// Manage saved profiles: API and console endpoints, default org
#[derive(Debug, Clone, clap::Subcommand)]
pub enum ProfileCommand {
    /// List your profiles
    #[command(name = "list")]
    List(super::NoArgs),

    /// Show one profile
    #[command(name = "show")]
    Show(NameArgs),

    /// Add a profile
    #[command(name = "add")]
    Add(ProfileAddArgs),

    /// Delete a profile
    #[command(name = "remove")]
    Remove(NameArgs),

    /// Make a profile the default
    #[command(name = "use")]
    Use(NameArgs),

    /// Change one setting in a profile
    #[command(name = "set")]
    Set(ProfileSetArgs),
}

impl ProfileCommand {
    /// Run one `profile` subcommand (store I/O only, so no async runtime).
    pub fn dispatch(&self, global: &GlobalArgs) -> Result<()> {
        match self {
            Self::List(_) => list(global),
            Self::Show(args) => show(global, args),
            Self::Add(args) => add(global, args),
            Self::Remove(args) => remove(args),
            Self::Use(args) => use_profile(args),
            Self::Set(args) => set(args),
        }
    }
}

/// `profile list`: every profile with where it points and whether it can sign in.
fn list(global: &GlobalArgs) -> Result<()> {
    let store = ProfileStore::load()?;
    let rows: Vec<Value> = store
        .config()
        .profiles
        .iter()
        .map(|(name, profile)| profile_row(&store, name, profile))
        .collect();
    print(global, &Value::Array(rows))
}

/// `profile show <name>`: the same row on its own.
fn show(global: &GlobalArgs, args: &NameArgs) -> Result<()> {
    let store = ProfileStore::load()?;
    require_known(&store, &args.name)?;
    let profile = store
        .profile(&args.name)
        .expect("require_known checked that the profile exists");
    print(global, &profile_row(&store, &args.name, profile))
}

/// `profile add <name>`: a new profile pointing somewhere.
fn add(global: &GlobalArgs, args: &ProfileAddArgs) -> Result<()> {
    let mut store = ProfileStore::load()?;
    if store.profile(&args.name).is_some() {
        return Err(Error::Usage(format!(
            "profile '{}' already exists; see it with: selfhost profile show {}",
            args.name, args.name
        )));
    }

    let base_url = args
        .base_url
        .clone()
        .or_else(|| global.base_url.clone())
        .ok_or_else(|| Error::Usage("where should this profile point? pass --base-url <url>".to_string()))?;
    validate_endpoint("base_url", &base_url)?;

    let console_url = match args.console_url.clone() {
        Some(url) => {
            validate_endpoint("console_url", &url)?;
            Some(url)
        }
        None => known_console_url(&base_url),
    };

    store.insert_profile(
        &args.name,
        Profile {
            base_url: base_url.clone(),
            console_url,
            org: None,
            provider: None,
            firebase_api_key: None,
            firebase_refresh_token: None,
            saved_at: None,
        },
    );
    store.save()?;

    println!("Added profile '{}' ({}).", args.name, base_url);
    Ok(())
}

/// `profile remove <name>`: delete a profile that is not the default.
fn remove(args: &NameArgs) -> Result<()> {
    let mut store = ProfileStore::load()?;
    require_known(&store, &args.name)?;
    if args.name == store.default_profile_name() {
        return Err(Error::Usage(format!(
            "cannot remove the default profile '{}'; switch first: selfhost profile use <other>",
            args.name
        )));
    }

    store.remove_profile(&args.name);
    store.save()?;

    println!("Removed profile '{}'.", args.name);
    Ok(())
}

/// `profile use <name>`: make a profile the default.
fn use_profile(args: &NameArgs) -> Result<()> {
    let mut store = ProfileStore::load()?;
    require_known(&store, &args.name)?;

    store.set_default_profile(&args.name);
    store.save()?;

    println!("Default profile is now '{}'.", args.name);
    Ok(())
}

/// `profile set <name> <key> <value>`: change one setting, validating endpoints
/// and providers; an empty value clears everything except the base URL.
fn set(args: &ProfileSetArgs) -> Result<()> {
    let mut store = ProfileStore::load()?;
    require_known(&store, &args.name)?;

    let key = args.key.as_str();
    let value = args.value.as_str();
    let profile = store
        .profile_mut(&args.name)
        .expect("require_known checked that the profile exists");

    match key {
        "base_url" => {
            if value.is_empty() {
                return Err(Error::Usage(
                    "base_url cannot be empty; pass the URL this profile should use".to_string(),
                ));
            }
            validate_endpoint("base_url", value)?;
            profile.base_url = value.to_string();
        }
        "console_url" => {
            if value.is_empty() {
                profile.console_url = None;
            } else {
                validate_endpoint("console_url", value)?;
                profile.console_url = Some(value.to_string());
            }
        }
        "org" => {
            profile.org = if value.is_empty() {
                None
            } else {
                Some(value.to_string())
            };
        }
        "provider" => {
            profile.provider = match value {
                "" => None,
                "aws" | "hetzner" => Some(value.to_string()),
                other => {
                    return Err(Error::Usage(format!(
                        "unknown provider '{other}'; use aws or hetzner, or pass an empty value to clear it"
                    )));
                }
            };
        }
        other => {
            return Err(Error::Usage(format!(
                "unknown setting '{other}'; use one of: base_url, console_url, org, provider"
            )));
        }
    }

    store.save()?;
    println!("Updated profile '{}': {key} = {value}", args.name);
    Ok(())
}

/// One profile as the listing/show shape; the credentials never appear here.
fn profile_row(store: &ProfileStore, name: &str, profile: &Profile) -> Value {
    json!({
        "name": name,
        "base_url": profile.base_url,
        "console_url": profile.console_url,
        "org": profile.org,
        "provider": profile.provider,
        "default": name == store.default_profile_name(),
        "signed_in": signed_in(profile),
    })
}

/// Whether the profile itself carries a credential pair. The environment pair
/// is not profile state, so it never shows up here (unlike the credential
/// source `auth status` reports).
fn signed_in(profile: &Profile) -> bool {
    profile
        .firebase_api_key
        .as_deref()
        .is_some_and(|value| !value.is_empty())
        && profile
            .firebase_refresh_token
            .as_deref()
            .is_some_and(|value| !value.is_empty())
}

/// Render a value in the resolved format on stdout.
fn print(global: &GlobalArgs, value: &Value) -> Result<()> {
    let format = Format::resolve(global.format, global.json);
    println!("{}", format.render(value)?);
    Ok(())
}

/// A profile must exist before it can be shown, changed or removed; nothing in
/// this command ever creates one implicitly.
fn require_known(store: &ProfileStore, name: &str) -> Result<()> {
    if store.profile(name).is_none() {
        return Err(Error::Usage(format!(
            "unknown profile '{name}'; see the ones you have: selfhost profile list"
        )));
    }
    Ok(())
}

/// The console URL that goes with a base URL: the production API host has one,
/// anything else (QA, staging or self-hosted) gets none because its console host
/// is environment-specific and not shipped in this public repo.
fn known_console_url(base_url: &str) -> Option<String> {
    match host_of(base_url) {
        "api.selfhost.dev" => Some(PROD_CONSOLE_URL.to_string()),
        _ => None,
    }
}

/// Host part of a URL body: scheme, user info, port and path removed.
fn host_of(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let authority = rest
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(rest)
        .rsplit('@')
        .next()
        .unwrap_or(rest);
    match authority.rsplit_once(':') {
        Some((host, port)) if port.chars().all(|c| c.is_ascii_digit()) => host,
        _ => authority,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(tag: &str) -> ProfileStore {
        let dir = std::env::temp_dir().join(format!(
            "selfhost-profile-{tag}-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        ProfileStore::at(dir.join(crate::config::STORE_FILE)).unwrap()
    }

    #[test]
    fn signed_in_reports_only_the_profiles_own_credentials() {
        let mut store = store("signed-in");
        let row = profile_row(&store, "qa", store.profile("qa").unwrap());
        assert_eq!(row["signed_in"], false);

        // One half is not a credential pair.
        store.profile_mut("qa").unwrap().firebase_api_key = Some("key".into());
        let row = profile_row(&store, "qa", store.profile("qa").unwrap());
        assert_eq!(row["signed_in"], false);

        store.profile_mut("qa").unwrap().firebase_refresh_token = Some("token".into());
        let row = profile_row(&store, "qa", store.profile("qa").unwrap());
        assert_eq!(row["signed_in"], true);
    }
}
