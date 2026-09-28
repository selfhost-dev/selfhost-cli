//! Profile store for `~/.selfhost/config.json` (design §5).
//!
//! Slice 0 ships the serde model only: no file I/O, no `0600` handling, no
//! flag/env/profile precedence resolution, no MCP import. Those land in the
//! config/auth slices and read these types.

#![allow(dead_code)] // Slice 0: the store is consumed by the auth/config slices.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Profile used when `--profile`/`SELFHOST_PROFILE` are absent.
pub const DEFAULT_PROFILE: &str = "default";

/// Names of the built-in profiles, seeded as ordinary profiles on first run (§5).
pub const BUILTIN_PROFILE_NAMES: [&str; 3] = ["prod", "qa", "local"];

/// Production API base URL.
pub const PROD_BASE_URL: &str = "https://api.selfhost.dev";

/// QA API base URL.
pub const QA_BASE_URL: &str = "https://qapi.selfhost.dev";

/// Local development API base URL (Rails on `localhost:3000`).
pub const LOCAL_BASE_URL: &str = "http://localhost:3000";

/// Production console URL, used for the browser-login hop.
pub const PROD_CONSOLE_URL: &str = "https://console.selfhost.dev";

/// QA console URL.
///
/// [INFERENCE: needs platform confirmation] No QA console hostname exists anywhere in
/// the ecosystem; `https://console.selfhost.dev` is the single known console host
/// (`selfhost-mcp/src/config.ts:9`), so QA reuses it until the platform owner confirms
/// a separate QA console.
pub const QA_CONSOLE_URL: &str = PROD_CONSOLE_URL;

/// Default API base URL (production).
pub const DEFAULT_BASE_URL: &str = PROD_BASE_URL;

/// Default console URL (production), used for the browser-login hop.
pub const DEFAULT_CONSOLE_URL: &str = PROD_CONSOLE_URL;

/// The built-in profile for `name`, or `None` if `name` is not a built-in.
///
/// Built-ins are ordinary profiles: seeding them into the store is a config-slice
/// concern (file I/O); this is the pure model.
///
/// `local` has no known console URL, so its [`Profile::console_url`] is `None`
/// ([INFERENCE: needs platform confirmation]).
pub fn builtin_profile(name: &str) -> Option<Profile> {
    let (base_url, console_url) = match name {
        "prod" => (PROD_BASE_URL, Some(PROD_CONSOLE_URL)),
        "qa" => (QA_BASE_URL, Some(QA_CONSOLE_URL)),
        "local" => (LOCAL_BASE_URL, None),
        _ => return None,
    };
    Some(Profile {
        base_url: base_url.to_string(),
        console_url: console_url.map(str::to_owned),
        org: None,
        provider: None,
        firebase_api_key: None,
        firebase_refresh_token: None,
        saved_at: None,
    })
}

/// Root of `~/.selfhost/config.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Store schema version.
    pub version: u32,
    /// Profile used when none is selected explicitly.
    pub default_profile: String,
    /// Profiles by name (`default`, `prod`, `qa`, `local`, …).
    pub profiles: BTreeMap<String, Profile>,
}

/// One named profile: where to talk, which org, and how to authenticate.
#[derive(Clone, Serialize, Deserialize)]
pub struct Profile {
    /// API base URL, e.g. `https://api.selfhost.dev`.
    pub base_url: String,
    /// Console URL used for the `/mcp-auth` browser hop; absent for built-in `local`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub console_url: Option<String>,
    /// Organization slug or pid, persisted by `selfhost org use`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub org: Option<String>,
    /// Preferred compute provider (`aws` | `hetzner`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    /// Firebase web API key used for refresh-token exchange.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub firebase_api_key: Option<String>,
    /// Rotated on every refresh; the file is written back with mode `0600`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub firebase_refresh_token: Option<String>,
    /// When the credentials were last written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub saved_at: Option<DateTime<Utc>>,
}
impl std::fmt::Debug for Profile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Profile")
            .field("base_url", &self.base_url)
            .field("console_url", &self.console_url)
            .field("org", &self.org)
            .field("provider", &self.provider)
            .field(
                "firebase_api_key",
                &self.firebase_api_key.as_ref().map(|_| "[REDACTED]"),
            )
            .field(
                "firebase_refresh_token",
                &self.firebase_refresh_token.as_ref().map(|_| "[REDACTED]"),
            )
            .field("saved_at", &self.saved_at)
            .finish()
    }
}

impl Config {
    /// Empty store with the documented schema version.
    pub fn empty() -> Self {
        Self {
            version: 1,
            default_profile: DEFAULT_PROFILE.to_string(),
            profiles: BTreeMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_profiles_resolve_to_their_endpoints() {
        for name in BUILTIN_PROFILE_NAMES {
            assert!(
                builtin_profile(name).is_some(),
                "built-in profile {name} must resolve"
            );
        }

        assert_eq!(builtin_profile("prod").unwrap().base_url, PROD_BASE_URL);
        assert_eq!(
            builtin_profile("qa").unwrap().base_url,
            "https://qapi.selfhost.dev"
        );
        assert_eq!(builtin_profile("local").unwrap().base_url, LOCAL_BASE_URL);

        assert_eq!(
            builtin_profile("prod").unwrap().console_url.as_deref(),
            Some(PROD_CONSOLE_URL)
        );
        assert_eq!(
            builtin_profile("qa").unwrap().console_url.as_deref(),
            Some(PROD_CONSOLE_URL)
        );
        assert_eq!(builtin_profile("local").unwrap().console_url, None);
    }

    #[test]
    fn unknown_profile_names_have_no_builtin() {
        for name in ["default", "staging", "", "PROD", "prod "] {
            assert!(
                builtin_profile(name).is_none(),
                "{name:?} must not resolve to a built-in"
            );
        }
    }

    fn canary_profile() -> Profile {
        Profile {
            base_url: PROD_BASE_URL.to_string(),
            console_url: Some(PROD_CONSOLE_URL.to_string()),
            org: Some("acme".to_string()),
            provider: Some("aws".to_string()),
            firebase_api_key: Some("canary-api-key-9f8e7d".to_string()),
            firebase_refresh_token: Some("canary-refresh-token-1a2b3c".to_string()),
            saved_at: None,
        }
    }

    #[test]
    fn debug_output_hides_profile_credentials() {
        let shown = format!("{:?}", canary_profile());
        assert!(!shown.contains("canary-api-key-9f8e7d"));
        assert!(!shown.contains("canary-refresh-token-1a2b3c"));
        assert!(shown.contains("[REDACTED]"));
    }

    #[test]
    fn debug_output_hides_credentials_through_config() {
        let mut store = Config::empty();
        store
            .profiles
            .insert("default".to_string(), canary_profile());
        let shown = format!("{store:?}");
        assert!(!shown.contains("canary-api-key-9f8e7d"));
        assert!(!shown.contains("canary-refresh-token-1a2b3c"));
    }
}
