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

/// Default API base URL (production).
pub const DEFAULT_BASE_URL: &str = "https://api.selfhost.dev";

/// Default console URL (production), used for the browser-login hop.
pub const DEFAULT_CONSOLE_URL: &str = "https://console.selfhost.dev";

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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    /// API base URL, e.g. `https://api.selfhost.dev`.
    pub base_url: String,
    /// Console URL used for the `/mcp-auth` browser hop.
    pub console_url: String,
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
