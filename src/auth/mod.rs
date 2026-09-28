//! Authentication engine (design §5): the MCP loopback handshake
//! (`${console_url}/mcp-auth?port=N` → `/callback?refresh_token=…&api_key=…`),
//! the `securetoken.googleapis.com` refresh-token exchange, the in-memory
//! ID-token cache with its 5-minute buffer, rotated-refresh-token persistence,
//! and the one-time import of the MCP server's credential file.
//!
//! Two deliberate deviations from the MCP server this mirrors:
//!
//! - a refresh failure outside [`login`] never opens a browser. The MCP server
//!   falls back to an interactive sign-in; a CLI run from a script or an agent
//!   must instead fail with [`Error::NotAuthenticated`] and an exit code that
//!   names what to run.
//! - the ID-token cache is keyed by profile name, not one process-wide slot,
//!   because one CLI process may talk to more than one profile.
//!
//! Credentials never reach the output: the sign-in URL carries only the
//! loopback port, callback request lines are not logged, and exchange
//! responses are never echoed.

mod loopback;
mod mcp;

use std::collections::HashMap;
use std::io::IsTerminal as _;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use chrono::Utc;
use serde::Deserialize;
use tokio::io::{AsyncBufReadExt as _, BufReader};

use crate::config::{
    ENV_FIREBASE_API_KEY, ENV_FIREBASE_REFRESH_TOKEN, Profile, ProfileStore, validate_endpoint,
};
use crate::error::{Error, Result};

/// Firebase endpoint that trades a refresh token for an ID token; the web API
/// key travels as a query parameter.
const TOKEN_URL: &str = "https://securetoken.googleapis.com/v1/token";
/// The exchange is given 30 seconds (design §5).
const EXCHANGE_TIMEOUT: Duration = Duration::from_secs(30);
/// A cached ID token is renewed once it is this close to expiry.
const EXPIRY_BUFFER: Duration = Duration::from_secs(5 * 60);
/// Firebase ID tokens last an hour; used when `expires_in` is absent or unparsable.
const DEFAULT_EXPIRES_IN: u64 = 3600;
/// Ceiling on a server-supplied `expires_in`, so an absurd value cannot build a
/// `Duration` that overflows `Instant + Duration` and panics.
const MAX_EXPIRES_IN: u64 = 24 * 60 * 60;
/// The fix every "not authenticated" message names.
const LOGIN_HINT: &str = "run selfhost auth login";

/// A Firebase credential pair: web API key + refresh token.
#[derive(Clone)]
pub(crate) struct Credentials {
    pub(crate) api_key: String,
    pub(crate) refresh_token: String,
}

/// How [`login`] should behave.
#[derive(Debug, Clone, Copy, Default)]
pub struct LoginOptions {
    /// Print the sign-in URL instead of opening a browser.
    pub no_browser: bool,
}

/// What a successful [`login`] left behind.
#[derive(Debug, Clone)]
pub struct LoginOutcome {
    /// Profile that received the credentials.
    pub profile: String,
    /// Whether the credentials came from the MCP server's credential file.
    pub imported_from_mcp: bool,
}

/// Where this profile's credentials would come from, if anywhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialSource {
    /// The `firebase_api_key`/`firebase_refresh_token` pair in the profile store.
    Profile,
    /// The `FIREBASE_API_KEY`/`FIREBASE_REFRESH_TOKEN` pair (headless CI/agents).
    Environment,
}

/// Full interactive sign-in: adopt the MCP server's credentials when the user
/// agrees, otherwise open the console's `/mcp-auth` page on a loopback port,
/// then exchange whatever we got for an ID token and save the pair — including
/// the rotated refresh token — into the profile.
///
/// `assume_yes` answers the MCP-import prompt affirmatively without asking.
pub async fn login(
    store: &mut ProfileStore,
    name: &str,
    options: LoginOptions,
    assume_yes: bool,
) -> Result<LoginOutcome> {
    let profile = store.require_profile(name)?.clone();
    validate_endpoint("base_url", &profile.base_url)?;
    let console_url = require_console_url(name, profile.console_url.as_deref())?;
    validate_endpoint("console_url", console_url)?;

    if should_offer_mcp_import(
        has_credentials(&profile),
        std::io::stdin().is_terminal(),
        assume_yes,
    ) && let Some(outcome) = try_mcp_import(store, name, assume_yes).await
    {
        return Ok(outcome);
    }

    let credentials = loopback::login(console_url, options.no_browser).await?;
    let exchange = exchange(&credentials.api_key, &credentials.refresh_token)
        .await
        .map_err(|err| {
            Error::Other(anyhow::anyhow!("sign-in could not be completed ({err}); {LOGIN_HINT}"))
        })?;
    save_credentials(store, name, credentials.api_key, exchange.refresh_token)?;
    cache_token(name, exchange.id_token, exchange.expires_in);

    Ok(LoginOutcome {
        profile: name.to_string(),
        imported_from_mcp: false,
    })
}

/// Sign out of `name`: drop the credential pair and when it was saved, keep
/// every other setting, and persist. `Ok(false)` means there was nothing to
/// clear.
pub fn logout(store: &mut ProfileStore, name: &str) -> Result<bool> {
    store.require_profile(name)?;
    let cleared = match store.profile_mut(name) {
        // Either half counts: a stray refresh token is credential material too.
        Some(profile)
            if profile.firebase_api_key.is_some() || profile.firebase_refresh_token.is_some() =>
        {
            profile.firebase_api_key = None;
            profile.firebase_refresh_token = None;
            profile.saved_at = None;
            true
        }
        _ => false,
    };
    if cleared {
        forget_token(name);
        store.save()?;
    }
    Ok(cleared)
}

/// A valid Firebase ID token for `name`.
///
/// The environment pair wins over the profile's credentials; a cached token is
/// reused until it comes within [`EXPIRY_BUFFER`] of expiry; otherwise the
/// refresh token is exchanged and, when the credentials came from the profile,
/// the rotated refresh token is written back. A refresh failure is reported as
/// [`Error::NotAuthenticated`] naming the fix instead of opening a browser (see
/// the module docs).
pub async fn id_token(store: &mut ProfileStore, name: &str) -> Result<String> {
    if let Some(token) = cached_token(name) {
        return Ok(token);
    }

    let profile = store.require_profile(name)?.clone();
    validate_endpoint("base_url", &profile.base_url)?;

    let resolved = match env_credentials() {
        Some((api_key, refresh_token)) => Some((CredentialSource::Environment, api_key, refresh_token)),
        None => profile_credentials(&profile)
            .map(|(api_key, refresh_token)| (CredentialSource::Profile, api_key, refresh_token)),
    };
    let Some((source, api_key, refresh_token)) = resolved else {
        return Err(Error::NotAuthenticated(format!(
            "profile '{name}' has no Firebase credentials; {LOGIN_HINT}"
        )));
    };

    let exchange = exchange(&api_key, &refresh_token)
        .await
        .map_err(|err| exchange_failure(name, err))?;

    if source == CredentialSource::Profile {
        if let Some(profile) = store.profile_mut(name) {
            profile.firebase_refresh_token = Some(exchange.refresh_token.clone());
            profile.saved_at = Some(Utc::now());
        }
        store.save()?;
    }
    cache_token(name, exchange.id_token.clone(), exchange.expires_in);

    Ok(exchange.id_token)
}

/// Where the credentials for `name` would come from, if anywhere. The
/// environment pair, when both halves are set, wins over the profile.
pub fn credential_source(store: &ProfileStore, name: &str) -> Option<CredentialSource> {
    if env_credentials().is_some() {
        return Some(CredentialSource::Environment);
    }
    store
        .profile(name)
        .filter(|profile| has_credentials(profile))
        .map(|_| CredentialSource::Profile)
}

/// One successful refresh-token exchange.
struct Exchange {
    id_token: String,
    refresh_token: String,
    expires_in: u64,
}

/// The response body of a refresh-token exchange. `expires_in` is a string in
/// Firebase's answer, so it is read as untyped JSON and parsed leniently.
#[derive(Deserialize)]
struct TokenResponse {
    id_token: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<serde_json::Value>,
}

/// Why a refresh-token exchange failed.
enum ExchangeError {
    /// `securetoken` answered 4xx: the refresh token itself was refused.
    Rejected(u16),
    /// The exchange never got a usable answer (transport, 5xx, bad body).
    Transport(anyhow::Error),
}

impl std::fmt::Display for ExchangeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Rejected(code) => write!(f, "Firebase rejected the credentials (HTTP {code})"),
            Self::Transport(err) => write!(f, "{err:#}"),
        }
    }
}

/// Turn an exchange failure into what the caller should report. A rejection
/// means "sign in again" (exit 3); a transport failure keeps its cause and exits
/// as a runtime error (exit 1).
fn exchange_failure(name: &str, err: ExchangeError) -> Error {
    match err {
        ExchangeError::Rejected(code) => Error::NotAuthenticated(format!(
            "the sign-in for profile '{name}' was rejected (HTTP {code}); {LOGIN_HINT}"
        )),
        ExchangeError::Transport(err) => {
            Error::Other(anyhow::anyhow!(
                "cannot renew the sign-in for profile '{name}': {err:#}"
            ))
        }
    }
}

/// POST the refresh token to Firebase and return the rotated pair.
async fn exchange(
    api_key: &str,
    refresh_token: &str,
) -> std::result::Result<Exchange, ExchangeError> {
    exchange_at(TOKEN_URL, api_key, refresh_token).await
}

/// [`exchange`] against an arbitrary endpoint, for tests.
async fn exchange_at(
    token_url: &str,
    api_key: &str,
    refresh_token: &str,
) -> std::result::Result<Exchange, ExchangeError> {
    let client = reqwest::Client::builder()
        .timeout(EXCHANGE_TIMEOUT)
        .build()
        .map_err(|err| {
            ExchangeError::Transport(anyhow::Error::from(err).context("cannot build the HTTP client"))
        })?;
    let response = client
        .post(format!("{token_url}?key={}", percent_encode(api_key)))
        .header(reqwest::header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(exchange_body(refresh_token))
        .send()
        .await
        .map_err(|err| {
            // `without_url` keeps the `?key=…` URL out of the error chain.
            ExchangeError::Transport(
                anyhow::Error::from(err.without_url())
                    .context("cannot reach securetoken.googleapis.com"),
            )
        })?;

    let status = response.status();
    if !status.is_success() {
        let code = status.as_u16();
        // The body is never surfaced: it can echo credential material.
        return Err(if status.is_client_error() {
            ExchangeError::Rejected(code)
        } else {
            ExchangeError::Transport(anyhow::anyhow!(
                "Firebase token exchange failed with HTTP {code}"
            ))
        });
    }
    let body = response.text().await.map_err(|err| {
        ExchangeError::Transport(
            anyhow::Error::from(err.without_url())
                .context("cannot read the token exchange response"),
        )
    })?;
    let parsed: TokenResponse = serde_json::from_str(&body).map_err(|_| {
        ExchangeError::Transport(anyhow::anyhow!(
            "Firebase token exchange returned an unexpected response"
        ))
    })?;

    let id_token = parsed
        .id_token
        .filter(|token| !token.is_empty())
        .ok_or_else(|| {
            ExchangeError::Transport(anyhow::anyhow!(
                "Firebase token exchange returned no ID token"
            ))
        })?;
    // Firebase always rotates the refresh token; if it ever does not, the one we
    // sent is still the pair's other half.
    let refresh_token = parsed
        .refresh_token
        .filter(|token| !token.is_empty())
        .unwrap_or_else(|| refresh_token.to_string());

    Ok(Exchange {
        id_token,
        refresh_token,
        expires_in: expires_in(parsed.expires_in.as_ref()),
    })
}

/// `application/x-www-form-urlencoded` body for one exchange.
fn exchange_body(refresh_token: &str) -> String {
    format!(
        "grant_type=refresh_token&refresh_token={}",
        percent_encode(refresh_token)
    )
}

/// Percent-encode everything but the RFC 3986 unreserved characters, so `+`,
/// `/`, `=` and `&` inside a token survive the form body intact.
fn percent_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            out.push('%');
            out.push(hex_digit(byte >> 4));
            out.push(hex_digit(byte & 0x0f));
        }
    }
    out
}

fn hex_digit(nibble: u8) -> char {
    char::from_digit(u32::from(nibble), 16)
        .expect("a nibble always has a hex digit")
        .to_ascii_uppercase()
}

/// `expires_in` arrives as a string; anything unparsable falls back to an hour,
/// and anything larger than a day is clamped to [`MAX_EXPIRES_IN`].
fn expires_in(value: Option<&serde_json::Value>) -> u64 {
    let seconds = match value {
        Some(serde_json::Value::String(text)) => text.parse().unwrap_or(DEFAULT_EXPIRES_IN),
        Some(serde_json::Value::Number(number)) => number.as_u64().unwrap_or(DEFAULT_EXPIRES_IN),
        _ => DEFAULT_EXPIRES_IN,
    };
    seconds.min(MAX_EXPIRES_IN)
}

/// Write the credential pair into the profile and persist it.
fn save_credentials(
    store: &mut ProfileStore,
    name: &str,
    api_key: String,
    refresh_token: String,
) -> Result<()> {
    let profile = store.profile_mut(name).ok_or_else(|| {
        Error::Usage(format!(
            "unknown profile '{name}'; create it first: selfhost profile add {name}"
        ))
    })?;
    profile.firebase_api_key = Some(api_key);
    profile.firebase_refresh_token = Some(refresh_token);
    profile.saved_at = Some(Utc::now());
    store.save()
}

/// Adopt the MCP server's credentials, if they exist and the exchange accepts
/// them. A rejected pair warns and leaves the caller to the browser flow; the
/// MCP file itself is never touched.
async fn try_mcp_import(store: &mut ProfileStore, name: &str, assume_yes: bool) -> Option<LoginOutcome> {
    let path = mcp::default_path()?;
    let credentials = mcp::load_from(&path).ok()?;
    if !assume_yes && !confirm_mcp_import(name).await {
        return None;
    }

    let exchange = match exchange(&credentials.api_key, &credentials.refresh_token).await {
        Ok(exchange) => exchange,
        Err(err) => {
            eprintln!(
                "warning: the credentials in {} could not be verified ({err}); continuing with browser sign-in",
                path.display()
            );
            return None;
        }
    };
    if let Err(err) = save_credentials(store, name, credentials.api_key, exchange.refresh_token) {
        eprintln!("warning: could not save the imported credentials ({err}); continuing with browser sign-in");
        return None;
    }
    cache_token(name, exchange.id_token, exchange.expires_in);

    Some(LoginOutcome {
        profile: name.to_string(),
        imported_from_mcp: true,
    })
}

/// Ask before adopting the MCP server's credentials.
async fn confirm_mcp_import(name: &str) -> bool {
    eprintln!("Found sign-in credentials from the SelfHost MCP server for profile '{name}'.");
    eprintln!("Import them into this profile? [Y/n]");
    let mut answer = String::new();
    match BufReader::new(tokio::io::stdin()).read_line(&mut answer).await {
        Ok(read) if read > 0 => {
            let answer = answer.trim().to_ascii_lowercase();
            answer.is_empty() || answer == "y" || answer == "yes"
        }
        _ => false,
    }
}

/// Whether [`login`] should look for the MCP credential file: only when the
/// profile has no credentials of its own, and only when there is someone to
/// ask (`assume_yes`) or a terminal to ask on.
fn should_offer_mcp_import(profile_has_credentials: bool, stdin_is_tty: bool, assume_yes: bool) -> bool {
    !profile_has_credentials && (stdin_is_tty || assume_yes)
}

/// The console URL for the browser hop, or the error naming the exact fix.
fn require_console_url<'a>(name: &str, console_url: Option<&'a str>) -> Result<&'a str> {
    console_url.filter(|url| !url.is_empty()).ok_or_else(|| {
        Error::Other(anyhow::anyhow!(
            "profile '{name}' has no console URL, so there is no sign-in page to open; \
             set one with selfhost profile set {name} console_url <url> \
             (the built-in local profile has none)"
        ))
    })
}

/// Both halves of the profile's credential pair, non-empty.
fn has_credentials(profile: &Profile) -> bool {
    non_empty(profile.firebase_api_key.as_deref()) && non_empty(profile.firebase_refresh_token.as_deref())
}

/// The profile's credential pair, when both halves are set.
fn profile_credentials(profile: &Profile) -> Option<(String, String)> {
    let api_key = profile.firebase_api_key.clone().filter(|key| !key.is_empty())?;
    let refresh_token = profile
        .firebase_refresh_token
        .clone()
        .filter(|token| !token.is_empty())?;
    Some((api_key, refresh_token))
}

fn non_empty(value: Option<&str>) -> bool {
    value.is_some_and(|value| !value.is_empty())
}

/// The `FIREBASE_API_KEY`/`FIREBASE_REFRESH_TOKEN` pair; both must be set.
fn env_credentials() -> Option<(String, String)> {
    let api_key = std::env::var(ENV_FIREBASE_API_KEY).ok().filter(|v| !v.is_empty())?;
    let refresh_token = std::env::var(ENV_FIREBASE_REFRESH_TOKEN)
        .ok()
        .filter(|v| !v.is_empty())?;
    Some((api_key, refresh_token))
}

/// One cached ID token and the instant it stops being worth using.
struct CachedToken {
    token: String,
    expires_at: Instant,
}

/// Process-global ID-token cache, one entry per profile name.
static TOKEN_CACHE: LazyLock<Mutex<HashMap<String, CachedToken>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// A token for `name` that is still comfortably valid.
fn cached_token(name: &str) -> Option<String> {
    let cache = TOKEN_CACHE.lock().ok()?;
    let entry = cache.get(name)?;
    token_is_fresh(entry.expires_at, Instant::now()).then(|| entry.token.clone())
}

/// Remember a freshly minted ID token for `name`.
fn cache_token(name: &str, token: String, expires_in: u64) {
    let expires_in = expires_in.min(MAX_EXPIRES_IN);
    if let Ok(mut cache) = TOKEN_CACHE.lock() {
        cache.insert(
            name.to_string(),
            CachedToken {
                token,
                expires_at: Instant::now() + Duration::from_secs(expires_in),
            },
        );
    }
}

/// Drop any cached token for `name` — a fresh sign-in may be a different account.
fn forget_token(name: &str) {
    if let Ok(mut cache) = TOKEN_CACHE.lock() {
        cache.remove(name);
    }
}

/// Whether a token expiring at `expires_at` is worth reusing at `now`: it must
/// still have more than the 5-minute buffer left.
fn token_is_fresh(expires_at: Instant, now: Instant) -> bool {
    expires_at.saturating_duration_since(now) > EXPIRY_BUFFER
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_DIR: AtomicU64 = AtomicU64::new(0);

    fn temp_dir(tag: &str) -> PathBuf {
        let unique = NEXT_DIR.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "selfhost-auth-{tag}-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn store_in(tag: &str) -> (ProfileStore, PathBuf) {
        let path = temp_dir(tag).join(crate::config::STORE_FILE);
        (ProfileStore::at(path.clone()).unwrap(), path)
    }

    #[test]
    fn credential_source_prefers_the_environment_pair_and_falls_back_to_the_profile() {
        let (mut store, _) = store_in("source");
        {
            let profile = store.profile_mut("qa").unwrap();
            profile.firebase_api_key = Some("profile-key".into());
            profile.firebase_refresh_token = Some("profile-token".into());
        }
        // The test process is the only writer of these two names.
        unsafe {
            std::env::remove_var(ENV_FIREBASE_API_KEY);
            std::env::remove_var(ENV_FIREBASE_REFRESH_TOKEN);
        }
        assert_eq!(credential_source(&store, "qa"), Some(CredentialSource::Profile));

        // One half of the environment pair is not a credential pair.
        unsafe { std::env::set_var(ENV_FIREBASE_API_KEY, "env-key") };
        assert_eq!(credential_source(&store, "qa"), Some(CredentialSource::Profile));
        assert_eq!(credential_source(&store, "local"), None);

        // Both halves win over the profile.
        unsafe { std::env::set_var(ENV_FIREBASE_REFRESH_TOKEN, "env-token") };
        assert_eq!(
            credential_source(&store, "qa"),
            Some(CredentialSource::Environment)
        );
        assert_eq!(
            credential_source(&store, "local"),
            Some(CredentialSource::Environment)
        );

        unsafe {
            std::env::remove_var(ENV_FIREBASE_API_KEY);
            std::env::remove_var(ENV_FIREBASE_REFRESH_TOKEN);
        }
        assert_eq!(credential_source(&store, "local"), None);
        assert_eq!(credential_source(&store, "unknown-profile"), None);
    }

    #[test]
    fn cached_id_tokens_are_reused_until_the_expiry_buffer_is_reached() {
        let now = Instant::now();

        assert!(token_is_fresh(now + Duration::from_secs(3600), now));
        assert!(token_is_fresh(now + EXPIRY_BUFFER + Duration::from_secs(1), now));
        assert!(!token_is_fresh(now + EXPIRY_BUFFER, now));
        assert!(!token_is_fresh(now + Duration::from_secs(60), now));
        assert!(!token_is_fresh(now, now));
        assert!(!token_is_fresh(now - Duration::from_secs(1), now));
    }

    #[test]
    fn a_fresh_sign_out_clears_the_credentials_and_keeps_the_settings() {
        let (mut store, path) = store_in("logout");
        {
            let profile = store.profile_mut("qa").unwrap();
            profile.org = Some("acme".into());
            profile.provider = Some("aws".into());
            profile.firebase_api_key = Some("profile-key".into());
            profile.firebase_refresh_token = Some("profile-token".into());
            profile.saved_at = Some(Utc::now());
        }
        store.save().unwrap();
        cache_token("qa", "still-cached".into(), 3600);

        assert!(logout(&mut store, "qa").unwrap());

        let profile = store.profile("qa").unwrap();
        assert_eq!(profile.org.as_deref(), Some("acme"));
        assert_eq!(profile.provider.as_deref(), Some("aws"));
        assert!(profile.firebase_api_key.is_none());
        assert!(profile.firebase_refresh_token.is_none());
        assert!(profile.saved_at.is_none());
        assert_eq!(cached_token("qa"), None);

        // The clear survived the round trip to disk, and a second sign-out is
        // a no-op rather than an error.
        let reloaded = ProfileStore::at(path).unwrap();
        assert!(reloaded.profile("qa").unwrap().firebase_refresh_token.is_none());
        assert!(!logout(&mut store, "qa").unwrap());

        // A half-configured pair is credential material worth clearing too.
        store.profile_mut("qa").unwrap().firebase_refresh_token = Some("stray".into());
        assert!(logout(&mut store, "qa").unwrap());
        assert!(store.profile("qa").unwrap().firebase_refresh_token.is_none());
    }

    #[test]
    fn the_mcp_import_offer_needs_a_credential_free_profile_and_someone_to_ask() {
        assert!(should_offer_mcp_import(false, true, false));
        assert!(should_offer_mcp_import(false, false, true));
        assert!(should_offer_mcp_import(false, true, true));
        assert!(!should_offer_mcp_import(true, true, true));
        assert!(!should_offer_mcp_import(false, false, false));
    }

    #[test]
    fn the_exchange_body_percent_encodes_the_refresh_token() {
        assert_eq!(
            exchange_body("AMf-1"),
            "grant_type=refresh_token&refresh_token=AMf-1"
        );
        assert_eq!(
            exchange_body("AMf-1/a+b=c &d"),
            "grant_type=refresh_token&refresh_token=AMf-1%2Fa%2Bb%3Dc%20%26d"
        );
        assert_eq!(
            exchange_body("ü"),
            "grant_type=refresh_token&refresh_token=%C3%BC"
        );
    }

    #[test]
    fn expires_in_reads_firebase_s_seconds_string_and_defaults_to_an_hour() {
        let string = serde_json::json!("1800");
        let number = serde_json::json!(900);
        let nonsense = serde_json::json!("soon");

        assert_eq!(expires_in(Some(&string)), 1800);
        assert_eq!(expires_in(Some(&number)), 900);
        assert_eq!(expires_in(Some(&nonsense)), DEFAULT_EXPIRES_IN);
        assert_eq!(expires_in(None), DEFAULT_EXPIRES_IN);
    }

    #[test]
    fn a_profile_without_a_console_url_names_the_exact_fix() {
        let (store, _) = store_in("console");
        let console = store.profile("local").unwrap().console_url.clone();

        let message = require_console_url("local", console.as_deref())
            .unwrap_err()
            .to_string();

        assert!(message.contains("selfhost profile set local console_url"), "{message}");
        assert!(message.contains("local profile has none"), "{message}");
    }

    #[test]
    fn a_huge_expires_in_is_clamped_instead_of_overflowing() {
        let huge = serde_json::json!("99999999999");
        let max = serde_json::json!(u64::MAX);

        assert_eq!(expires_in(Some(&huge)), MAX_EXPIRES_IN);
        assert_eq!(expires_in(Some(&max)), MAX_EXPIRES_IN);
        assert_eq!(expires_in(None), DEFAULT_EXPIRES_IN);

        // And caching it must not panic on `Instant + Duration`.
        cache_token("clamp-test", "token".into(), u64::MAX);
        assert_eq!(cached_token("clamp-test").as_deref(), Some("token"));
        forget_token("clamp-test");
    }

    #[test]
    fn exchange_failures_keep_rejection_and_transport_apart() {
        let rejected = exchange_failure("qa", ExchangeError::Rejected(400));
        assert!(matches!(rejected, Error::NotAuthenticated(_)));
        assert_eq!(rejected.exit_code(), 3);
        assert!(rejected.to_string().contains("selfhost auth login"), "{rejected}");

        let transport =
            exchange_failure("qa", ExchangeError::Transport(anyhow::anyhow!("connection refused")));
        assert!(matches!(transport, Error::Other(_)));
        assert_eq!(transport.exit_code(), 1);
        assert!(transport.to_string().contains("connection refused"), "{transport}");
    }

    #[tokio::test]
    async fn exchange_transport_errors_never_carry_the_key_url() {
        // A port with nothing listening: reqwest's error would name the URL,
        // `?key=…` included, if `without_url` were not applied.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);

        let err = match exchange_at(&format!("http://{addr}"), "super-secret-key", "token").await {
            Err(err) => err,
            Ok(_) => panic!("the connection must fail"),
        };
        let text = err.to_string();

        assert!(!text.contains("super-secret-key"), "{text}");
        assert!(!text.contains("key="), "{text}");
    }

    #[tokio::test]
    async fn a_rejected_exchange_is_a_client_error() {
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 1024];
            let _ = socket.read(&mut buf).await;
            let body = r#"{"error":{"message":"INVALID_REFRESH_TOKEN"}}"#;
            let response = format!(
                "HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            socket.write_all(response.as_bytes()).await.unwrap();
            socket.shutdown().await.ok();
        });

        let err = match exchange_at(&format!("http://{addr}"), "key", "token").await {
            Err(err) => err,
            Ok(_) => panic!("a 400 must be rejected"),
        };
        assert!(matches!(err, ExchangeError::Rejected(400)), "{err}");

        server.await.unwrap();
    }
}
