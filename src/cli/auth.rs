//! `auth` — sign in/out, inspect the session (design §5).

use anyhow::anyhow;
use clap::Args;
use serde_json::{Value, json};

use crate::api::ApiClient;
use crate::auth::{self, CredentialSource, LoginOptions};
use crate::config::{ProfileStore, STORE_DIR, STORE_FILE};
use crate::error::{Error, Result};
use crate::output::Format;

use super::{GlobalArgs, NoArgs, block_on, effective_base_url};

// `auth login [--no-browser]` — browser OAuth through `${console_url}/mcp-auth`.
#[derive(Debug, Clone, Args)]
pub struct LoginArgs {
    /// Print the sign-in URL instead of opening a browser
    #[arg(long = "no-browser")]
    pub no_browser: bool,
}

/// Sign in, sign out and inspect the current session
#[derive(Debug, Clone, clap::Subcommand)]
pub enum AuthCommand {
    /// Sign in through your browser
    #[command(name = "login")]
    Login(LoginArgs),

    /// Sign out of the current profile
    #[command(name = "logout")]
    Logout(NoArgs),

    /// Show who you are signed in as
    #[command(name = "status")]
    Status(NoArgs),

    /// Print the current access token
    #[command(name = "token")]
    Token(NoArgs),
}

impl AuthCommand {
    /// Run one `auth` subcommand against the global options.
    pub fn dispatch(&self, global: &GlobalArgs) -> Result<()> {
        match self {
            Self::Login(args) => block_on(login(global, args)),
            Self::Logout(_) => block_on(logout(global)),
            Self::Status(_) => block_on(status(global)),
            Self::Token(_) => block_on(token(global)),
        }
    }
}

/// `auth login`: credentials first, platform account second.
///
/// The account is created with an unscoped `POST /users` after the credentials
/// are saved, so a provisioning failure still leaves a signed-in profile that
/// `auth status` can retry from.
async fn login(global: &GlobalArgs, args: &LoginArgs) -> Result<()> {
    let mut store = ProfileStore::load()?;
    let selected = store.resolved_name(global.profile.as_deref())?;
    // Validate the effective base URL before any sign-in work, so a bad
    // override fails closed instead of after a browser has been opened.
    let base_url = effective_base_url(global, store.require_profile(&selected)?)?;
    let outcome = auth::login(
        &mut store,
        &selected,
        LoginOptions {
            no_browser: args.no_browser,
        },
        global.yes,
    )
    .await?;
    // The outcome names the profile the credentials landed in.
    let name = outcome.profile;
    let imported_from_mcp = outcome.imported_from_mcp;

    // Cached by the sign-in above, so this costs no extra exchange.
    let token = auth::id_token(&mut store, &name).await?;
    let email = token_claims(&token)
        .and_then(|claims| claims.get("email").and_then(Value::as_str).map(str::to_owned));
    let signed_in = signed_in_line(&name, email.as_deref());

    if imported_from_mcp {
        println!("Imported credentials from the MCP setup into profile '{name}'.");
    }

    let client = ApiClient::with_token(&base_url, token, global.timeout)
        .with_verbosity(global.verbose, global.debug);

    let data = match client.post_unscoped("/users", json!({})).await {
        Ok(data) => data,
        Err(err) => {
            println!("{signed_in}");
            return Err(Error::Other(anyhow!(
                "signed in, but account setup failed: {err}; credentials are saved — retry with selfhost auth status"
            )));
        }
    };

    // The account record names the address too; prefer it over the token claim.
    let email = data
        .get("email")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or(email);
    let signed_in = signed_in_line(&name, email.as_deref());

    let created = data.get("created").and_then(Value::as_bool).unwrap_or(false);
    println!(
        "{signed_in}{}",
        if created { " (account created)" } else { "" }
    );

    // Signing in never moves the default; say how to reach this profile.
    if name != store.default_profile_name() {
        println!("Use --profile {name}, or make it default: selfhost profile use {name}");
    }
    Ok(())
}

/// `auth logout`: drop the profile's credentials.
async fn logout(global: &GlobalArgs) -> Result<()> {
    let mut store = ProfileStore::load()?;
    let name = store.resolved_name(global.profile.as_deref())?;

    let cleared = auth::logout(&mut store, &name)?;
    store.save()?;

    if cleared {
        println!("Signed out of profile '{name}'.");
    } else {
        println!("Profile '{name}' was not signed in.");
    }
    Ok(())
}

/// `auth status`: a checklist of what the selected profile can do, rendered in
/// the requested format. Failures exit non-zero, warnings do not.
async fn status(global: &GlobalArgs) -> Result<()> {
    let mut store = ProfileStore::load()?;
    let selected = global.profile.is_some();
    let name = store.resolved_name(global.profile.as_deref())?;
    let profile = store.require_profile(&name)?.clone();
    // Fails before any request is built when the override is not a valid endpoint.
    let base_url = effective_base_url(global, &profile)?;

    let mut rows = vec![
        check("store", "ok", store_path(&store)),
        check(
            "profile",
            "ok",
            format!(
                "{} ({})",
                name.to_uppercase(),
                if selected { "selected" } else { "default" }
            ),
        ),
        check("api", "ok", base_url.clone()),
        check(
            "console",
            "ok",
            profile
                .console_url
                .clone()
                .unwrap_or_else(|| "not set (browser sign-in unavailable)".to_string()),
        ),
    ];

    // The first failing check decides the process exit code; every failure keeps
    // its own kind (billing 4, rate-limited 75, runtime 1, otherwise 3), so a
    // 500 or DNS failure on `/users/details` no longer reads as "not signed in".
    let mut first_failure: Option<Error> = None;

    let source = auth::credential_source(&store, &name);
    rows.push(match source {
        Some(CredentialSource::Profile) => check("credentials", "ok", "saved in the profile"),
        Some(CredentialSource::Environment) => {
            check("credentials", "ok", "from the environment (FIREBASE_API_KEY)")
        }
        None => {
            first_failure = Some(Error::NotAuthenticated(
                "not signed in — run: selfhost auth login".to_string(),
            ));
            check(
                "credentials",
                "fail",
                "not signed in — run: selfhost auth login",
            )
        }
    });

    let mut client = None;
    let mut verified = None;
    if source.is_some() {
        match auth::id_token(&mut store, &name).await {
            Ok(token) => {
                rows.push(check("token", "ok", "valid"));
                verified = token_claims(&token)
                    .and_then(|claims| claims.get("email_verified").and_then(Value::as_bool));
                client = Some(
                    ApiClient::with_token(&base_url, token, global.timeout)
                        .with_verbosity(global.verbose, global.debug),
                );
            }
            Err(err) => {
                // The real message, not a fabricated "session expired": a
                // rejected exchange is not authenticated (3), a transport
                // failure stays a runtime error (1).
                rows.push(check("token", "fail", err.to_string()));
                if first_failure.is_none() {
                    first_failure = Some(err);
                }
            }
        }
    }

    if let Some(client) = &client {
        match client.get_unscoped("/users/details", &[]).await {
            Ok(details) => {
                rows.push(check("account", "ok", account_detail(&details)));
                if let Some(flag) = details.get("email_verified").and_then(Value::as_bool) {
                    verified = Some(flag);
                }
            }
            Err(Error::NotAuthenticated(_)) => {
                rows.push(check(
                    "account",
                    "fail",
                    "no platform account — run: selfhost auth login",
                ));
                if first_failure.is_none() {
                    first_failure = Some(Error::NotAuthenticated(
                        "no platform account — run: selfhost auth login".to_string(),
                    ));
                }
            }
            Err(err) => {
                rows.push(check("account", "fail", err.to_string()));
                if first_failure.is_none() {
                    first_failure = Some(err);
                }
            }
        }

        // Only when the token says so: the row never claims an address is
        // verified on the strength of a missing flag.
        if let Some(verified) = verified {
            rows.push(if verified {
                check("email", "ok", "verified")
            } else {
                check(
                    "email",
                    "warn",
                    "not verified — creating an organization is blocked until you confirm your address",
                )
            });
        }
    }

    match profile.org.clone() {
        None => rows.push(check(
            "organization",
            "warn",
            "no organization chosen — run: selfhost org use <slug>",
        )),
        Some(org) => {
            let (row, failure) = organization_check(client.as_ref(), &org).await;
            rows.push(row);
            if first_failure.is_none() {
                first_failure = failure;
            }
        }
    }

    let format = Format::resolve(global.format, global.json);
    println!("{}", format.render(&Value::Array(rows.clone()))?);

    match status_error(first_failure, &rows) {
        Some(err) => Err(err),
        None => Ok(()),
    }
}

/// The error `auth status` exits with: the first failing check's own kind,
/// pointing at the rendered checklist. A failure with no error of its own (a
/// fail row such as a missing organization) is a plain runtime error. A run
/// whose only rows fail at `warn` exits zero.
fn status_error(first_failure: Option<Error>, rows: &[Value]) -> Option<Error> {
    if let Some(err) = first_failure {
        return Some(match err {
            Error::NotAuthenticated(message) => {
                Error::NotAuthenticated(format!("{message} (see the failed checks above)"))
            }
            Error::BillingRequired(message) => {
                Error::BillingRequired(format!("{message} (see the failed checks above)"))
            }
            Error::RateLimited(message) => {
                Error::RateLimited(format!("{message} (see the failed checks above)"))
            }
            Error::Other(err) => Error::Other(anyhow::anyhow!(
                "auth status: see the failed checks above: {err:#}"
            )),
            other => other,
        });
    }
    rows.iter()
        .find(|row| row.get("level").and_then(Value::as_str) == Some("fail"))
        .map(|failed| {
            let which = failed
                .get("check")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            Error::Other(anyhow!("auth status: {which} failed"))
        })
}

/// `auth token`: the raw access token, on stdout only.
async fn token(global: &GlobalArgs) -> Result<()> {
    let mut store = ProfileStore::load()?;
    let name = store.resolved_name(global.profile.as_deref())?;

    let token = auth::id_token(&mut store, &name).await?;
    println!("{token}");
    if !global.quiet {
        eprintln!("This token expires shortly; never log it or paste it into an issue.");
    }
    Ok(())
}

/// One checklist row.
fn check(check: &str, level: &str, detail: impl Into<String>) -> Value {
    json!({"check": check, "level": level, "detail": detail.into()})
}

/// Where the store lives, collapsed to `~/.selfhost/config.json` when it is the default.
fn store_path(store: &ProfileStore) -> String {
    match ProfileStore::default_path() {
        Ok(default) if default == store.path() => format!("~/{STORE_DIR}/{STORE_FILE}"),
        _ => store.path().display().to_string(),
    }
}

/// `<email> (<display name>)` from the account details.
fn account_detail(details: &Value) -> String {
    let email = details.get("email").and_then(Value::as_str);
    let display = details
        .get("display_name")
        .and_then(Value::as_str)
        .or_else(|| details.get("name").and_then(Value::as_str));
    match (email, display) {
        (Some(email), Some(display)) => format!("{email} ({display})"),
        (Some(email), None) => email.to_string(),
        (None, Some(display)) => display.to_string(),
        (None, None) => "signed in".to_string(),
    }
}

/// `<name> (<pid>)` for the membership named by pid or slug.
fn organization_detail(organizations: &Value, wanted: &str) -> Option<String> {
    let organization = super::find_organization(organizations, wanted)?;
    let pid = super::organization_pid(organization).unwrap_or_default();
    let slug = organization
        .get("slug")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let name = organization
        .get("name")
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty())
        .unwrap_or(slug);
    Some(format!("{name} ({pid})"))
}

/// The organization row: the configured org must be one of the memberships.
/// The second half is the failure that decides the exit code, when there is one.
async fn organization_check(client: Option<&ApiClient>, org: &str) -> (Value, Option<Error>) {
    let Some(client) = client else {
        return (
            check(
                "organization",
                "fail",
                "not signed in — run: selfhost auth login",
            ),
            Some(Error::NotAuthenticated(
                "not signed in — run: selfhost auth login".to_string(),
            )),
        );
    };
    match client.get_unscoped("/organizations", &[]).await {
        Ok(organizations) => match organization_detail(&organizations, org) {
            Some(detail) => (check("organization", "ok", detail), None),
            None => (
                check(
                    "organization",
                    "fail",
                    format!("organization '{org}' is not in your memberships"),
                ),
                None,
            ),
        },
        Err(err) => {
            let detail = err.to_string();
            (check("organization", "fail", detail), Some(err))
        }
    }
}

/// The line `auth login` prints once the credentials are stored.
fn signed_in_line(name: &str, email: Option<&str>) -> String {
    match email {
        Some(email) => format!("Signed in to profile '{name}' as {email}"),
        None => format!("Signed in to profile '{name}'"),
    }
}

/// The claims inside an ID token (a JWT).
///
/// The CLI reads `email`/`email_verified` to address the user in its own
/// output; the platform is what checks the signature, so none is verified here.
fn token_claims(token: &str) -> Option<Value> {
    let payload = token.split('.').nth(1)?;
    serde_json::from_slice(&base64url_decode(payload)?).ok()
}

/// Decode base64url (RFC 4648 §5), padding optional.
fn base64url_decode(input: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(input.len() / 4 * 3);
    let mut buffer = 0u32;
    let mut bits = 0u32;
    for byte in input.bytes() {
        let sextet = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'-' | b'+' => 62,
            b'_' | b'/' => 63,
            b'=' => break,
            _ => return None,
        };
        buffer = (buffer << 6) | u32::from(sextet);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_claims_read_the_email_a_login_reports() {
        // {"email":"aziz@example.com","email_verified":true}
        let token = "header.eyJlbWFpbCI6ImF6aXpAZXhhbXBsZS5jb20iLCJlbWFpbF92ZXJpZmllZCI6dHJ1ZX0.signature";
        let claims = token_claims(token).expect("the payload is readable JSON");
        assert_eq!(claims["email"], "aziz@example.com");
        assert_eq!(claims["email_verified"], true);

        assert!(token_claims("not-a-token").is_none());
        assert!(token_claims("header.!!!.signature").is_none());
    }

    #[test]
    fn status_exit_code_follows_the_first_failure_kind() {
        let fail_rows = vec![check("account", "fail", "server exploded")];

        // Credentials/token/401 -> not authenticated (3).
        let err = status_error(
            Some(Error::NotAuthenticated("nope".to_string())),
            &fail_rows,
        )
        .unwrap();
        assert_eq!(err.exit_code(), 3);

        // Anything else keeps its own kind.
        let err = status_error(
            Some(Error::BillingRequired("pay up".to_string())),
            &fail_rows,
        )
        .unwrap();
        assert_eq!(err.exit_code(), 4);

        let err = status_error(Some(Error::RateLimited("slow down".to_string())), &fail_rows).unwrap();
        assert_eq!(err.exit_code(), 75);

        let err = status_error(Some(Error::Other(anyhow!("DNS failure"))), &fail_rows).unwrap();
        assert_eq!(err.exit_code(), 1);
        assert!(err.to_string().contains("failed checks above"));
    }

    #[test]
    fn status_without_a_failure_exits_zero_and_flags_bare_fail_rows() {
        // Warn-only rows are not failures.
        let warns = vec![check("email", "warn", "not verified")];
        assert!(status_error(None, &warns).is_none());

        // A fail row with no error of its own is a runtime failure.
        let fail_rows = vec![check("organization", "fail", "not a member")];
        let err = status_error(None, &fail_rows).unwrap();
        assert_eq!(err.exit_code(), 1);
        assert!(err.to_string().contains("organization"));
    }
}
