//! `org` — organizations, members, invitations, activity log (design §4).
//!
//! `list`, `show`, `use`, `members list` and `activity list` are real; the
//! rest of the group keeps the Slice 0 staging error until its own slice lands.
//!
//! Every org-scoped request goes through the shared rule: a positional
//! organization, then `--org`/`SELFHOSTDEV_ORG`, then the profile's stored org,
//! resolved to a pid (`org_<hex>` used as-is, anything else an exact slug
//! lookup). `org list` never resolves the selected org and `org use` takes its
//! organization only from the command line: the listing has to survive a stale
//! one, and `use` is how a new one is chosen.

use std::collections::HashMap;

use clap::Args;
use serde_json::{Map, Value};

use crate::api::ApiClient;
use crate::config::ProfileStore;
use crate::error::{Error, Result};
use crate::output::strip_control_characters;

use super::{
    GlobalArgs, NoArgs, block_on, find_organization, org_client, organization_matches,
    organization_pid, print, stub_group, unknown_organization, unscoped_client,
};

// `org create <name> [--slug]`.
#[derive(Debug, Clone, Args)]
pub struct OrgCreateArgs {
    /// Organization name
    pub name: String,

    /// Slug to claim
    #[arg(long)]
    pub slug: Option<String>,
}

// Members and invitations share this shape.
#[derive(Debug, Clone, Args)]
pub struct MemberArgs {
    /// Email address of the person
    pub email: Option<String>,

    /// Role to grant (owner, admin, member, viewer)
    #[arg(long)]
    pub role: Option<String>,
}

// `org show [ORG]` / `org members list [ORG]` — the optional organization.
#[derive(Debug, Clone, Args)]
pub struct OrgTargetArgs {
    /// Organization slug or pid (default: the one you have selected)
    #[arg(value_name = "ORG")]
    pub target: Option<String>,
}

// `org use <ORG>` — the organization `org use` switches to.
#[derive(Debug, Clone, Args)]
pub struct OrgUseArgs {
    /// Organization slug or pid
    pub org: String,
}

// `org activity list [ORG] [--page N] [--limit N]`.
#[derive(Debug, Clone, Args)]
pub struct OrgActivityArgs {
    /// Organization slug or pid (default: the one you have selected)
    #[arg(value_name = "ORG")]
    pub target: Option<String>,

    /// Page number
    #[arg(long, default_value_t = 1)]
    pub page: u32,

    /// Entries per page, up to 200
    #[arg(
        long,
        default_value_t = 20,
        value_parser = clap::value_parser!(u32).range(1..=200)
    )]
    pub limit: u32,
}

/// People in your organization and their roles
#[derive(Debug, Clone, clap::Subcommand)]
pub enum OrgMembersCommand {
    /// List the people in an organization
    #[command(name = "list")]
    List(OrgTargetArgs),

    /// Add a member
    #[command(name = "add")]
    Add(MemberArgs),

    /// Remove a member
    #[command(name = "remove")]
    Remove(MemberArgs),

    /// Change a member's role
    #[command(name = "update-role")]
    UpdateRole(MemberArgs),
}

impl OrgMembersCommand {
    /// Run one `org members` subcommand against the global options.
    pub fn dispatch(&self, global: &GlobalArgs) -> Result<()> {
        match self {
            Self::List(args) => block_on(members(global, args)),
            Self::Add(_) => Err(Error::not_implemented("org members add")),
            Self::Remove(_) => Err(Error::not_implemented("org members remove")),
            Self::UpdateRole(_) => Err(Error::not_implemented("org members update-role")),
        }
    }
}

stub_group!(
    /// Roles you can grant
    OrgRolesCommand, "org roles",
    leaves {
        /// List roles
        List(NoArgs) => "list",
    }
    groups {}
);

stub_group!(
    /// Pending invitations
    OrgInvitesCommand, "org invites",
    leaves {
        /// List pending invitations
        List(NoArgs) => "list",
        /// Invite someone
        Create(MemberArgs) => "create",
        /// Cancel an invitation
        Revoke(MemberArgs) => "revoke",
    }
    groups {}
);

/// What happened in your organization
#[derive(Debug, Clone, clap::Subcommand)]
pub enum OrgActivityCommand {
    /// Show recent activity in an organization
    #[command(name = "list")]
    List(OrgActivityArgs),
}

impl OrgActivityCommand {
    /// Run one `org activity` subcommand against the global options.
    pub fn dispatch(&self, global: &GlobalArgs) -> Result<()> {
        match self {
            Self::List(args) => block_on(activity(global, args)),
        }
    }
}

/// Organizations, members, invitations and activity
#[derive(Debug, Clone, clap::Subcommand)]
pub enum OrgCommand {
    /// List the organizations you belong to
    #[command(name = "list")]
    List(NoArgs),

    /// Create an organization
    #[command(name = "create")]
    Create(OrgCreateArgs),

    /// Show details for an organization
    #[command(name = "show")]
    Show(OrgTargetArgs),

    /// Change an organization's details
    #[command(name = "update")]
    Update(super::TargetArgs),

    /// Delete an organization
    #[command(name = "delete")]
    Delete(super::TargetArgs),

    /// Choose the organization other commands use by default
    #[command(name = "use")]
    Use(OrgUseArgs),

    /// People in your organization and their roles
    #[command(name = "members")]
    #[command(subcommand)]
    Members(OrgMembersCommand),

    /// Roles you can grant
    #[command(name = "roles")]
    #[command(subcommand)]
    Roles(OrgRolesCommand),

    /// Pending invitations
    #[command(name = "invites")]
    #[command(subcommand)]
    Invites(OrgInvitesCommand),

    /// What happened in your organization
    #[command(name = "activity")]
    #[command(subcommand)]
    Activity(OrgActivityCommand),
}

impl OrgCommand {
    /// Run one `org` subcommand against the global options.
    pub fn dispatch(&self, global: &GlobalArgs) -> Result<()> {
        match self {
            Self::List(_) => block_on(list(global)),
            Self::Show(args) => block_on(show(global, args)),
            Self::Use(args) => block_on(use_organization(global, args)),
            Self::Members(command) => command.dispatch(global),
            Self::Activity(command) => command.dispatch(global),
            Self::Roles(command) => command.dispatch(),
            Self::Invites(command) => command.dispatch(),
            Self::Create(_) => Err(Error::not_implemented("org create")),
            Self::Update(_) => Err(Error::not_implemented("org update")),
            Self::Delete(_) => Err(Error::not_implemented("org delete")),
        }
    }
}

/// `org list`: every organization the caller belongs to, plus invitations still
/// pending. Unscoped and never resolved — the stored org may be stale or
/// unknown, and the listing still has to work. The profile's org marks the row
/// other commands would use.
async fn list(global: &GlobalArgs) -> Result<()> {
    let mut store = ProfileStore::load()?;
    let name = store.resolved_name(global.profile.as_deref())?;
    let selected = store.require_profile(&name)?.org.clone();
    let client = unscoped_client(&mut store, &name, global).await?;

    let selected = selected.as_deref().filter(|org| !org.is_empty());
    let rows = membership_listing(&client, selected, &name).await?;
    print(global, &Value::Array(rows))
}

/// `org show [ORG]`: one curated organization, never the raw payload.
async fn show(global: &GlobalArgs, args: &OrgTargetArgs) -> Result<()> {
    let mut store = ProfileStore::load()?;
    let name = store.resolved_name(global.profile.as_deref())?;
    let (client, pid) = org_client(&mut store, &name, global, args.target.as_deref()).await?;

    let summary = organization_show(&client, &pid, &name).await?;
    print(global, &summary)
}

/// `org use <ORG>`: verify the target is one of the caller's organizations and
/// remember its pid, never the slug it was named by.
async fn use_organization(global: &GlobalArgs, args: &OrgUseArgs) -> Result<()> {
    let mut store = ProfileStore::load()?;
    let name = store.resolved_name(global.profile.as_deref())?;
    let client = unscoped_client(&mut store, &name, global).await?;

    let organizations = client.get_unscoped("/organizations", &[]).await?;
    let organization = find_organization(&organizations, &args.org)
        .ok_or_else(|| unknown_organization(&args.org))?;
    let pid = organization_pid(organization)
        .ok_or_else(|| Error::Other(anyhow::anyhow!("organization '{}' has no pid", args.org)))?
        .to_string();

    store
        .profile_mut(&name)
        .expect("resolved_name checked that the profile exists")
        .org = Some(pid.clone());
    store.save()?;

    println!("{}", use_confirmation(organization, &args.org, &pid));
    Ok(())
}

/// The `org use` confirmation line. The name is the organization's own — the
/// server's text, and the pid is echoed too — so both are stripped of
/// terminal-driving and bidi characters before the line is printed. `requested`
/// (what the user typed) is the fallback when the payload carries no name or
/// slug.
fn use_confirmation(organization: &Value, requested: &str, pid: &str) -> String {
    let shown = organization
        .get("name")
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty())
        .or_else(|| organization.get("slug").and_then(Value::as_str))
        .unwrap_or(requested);
    format!(
        "Now using {} ({})",
        strip_control_characters(shown),
        strip_control_characters(pid)
    )
}

/// `org members list [ORG]`: the people in one organization.
async fn members(global: &GlobalArgs, args: &OrgTargetArgs) -> Result<()> {
    let mut store = ProfileStore::load()?;
    let name = store.resolved_name(global.profile.as_deref())?;
    let (client, pid) = org_client(&mut store, &name, global, args.target.as_deref()).await?;

    let rows = member_listing(&client, &pid).await?;
    print(global, &Value::Array(rows))
}

/// `org activity list [ORG]`: recent entries from one organization's log.
async fn activity(global: &GlobalArgs, args: &OrgActivityArgs) -> Result<()> {
    let mut store = ProfileStore::load()?;
    let name = store.resolved_name(global.profile.as_deref())?;
    let (client, pid) = org_client(&mut store, &name, global, args.target.as_deref()).await?;

    let rows = activity_listing(&client, &pid, args.page, args.limit).await?;
    print(global, &Value::Array(rows))
}

/// `GET /users/memberships` — unscoped, because the listing is exactly the
/// caller's identity view and has to work when the stored org is gone.
async fn membership_listing(
    client: &ApiClient,
    selected: Option<&str>,
    profile: &str,
) -> Result<Vec<Value>> {
    let memberships = client.get_unscoped("/users/memberships", &[]).await?;
    let organizations = client.get_unscoped("/organizations", &[]).await?;
    let memberships = with_organization_slugs(&memberships, &organizations);
    Ok(membership_rows(&memberships, selected, profile))
}

/// `GET /organizations/:pid`, curated for `org show`.
async fn organization_show(client: &ApiClient, pid: &str, profile: &str) -> Result<Value> {
    let organization = client.get(&format!("/organizations/{pid}"), &[]).await?;
    Ok(organization_summary(&organization, profile))
}

/// `GET /organizations/:pid/members`.
async fn member_listing(client: &ApiClient, pid: &str) -> Result<Vec<Value>> {
    let members = client
        .get(&format!("/organizations/{pid}/members"), &[])
        .await?;
    Ok(member_rows(&members))
}

/// `GET /organizations/:pid/activity_logs` for one page.
async fn activity_listing(
    client: &ApiClient,
    pid: &str,
    page: u32,
    limit: u32,
) -> Result<Vec<Value>> {
    let page = page.to_string();
    let limit = limit.to_string();
    let query: [(&str, &str); 2] = [("page", page.as_str()), ("limit", limit.as_str())];
    let logs = client
        .get(&format!("/organizations/{pid}/activity_logs"), &query)
        .await?;
    Ok(activity_rows(&logs))
}

/// Merge the slug from `GET /organizations` into each membership's organization
/// where the memberships payload carries none, keyed by pid. An invitation whose
/// organization is absent from the lookup (a pending one) legitimately stays
/// slug-less: the key is omitted, never null.
fn with_organization_slugs(memberships: &Value, organizations: &Value) -> Value {
    let mut slugs: HashMap<String, String> = HashMap::new();
    if let Some(organizations) = organizations.as_array() {
        for organization in organizations {
            if let (Some(pid), Some(slug)) = (
                organization_pid(organization),
                organization.get("slug").and_then(Value::as_str),
            ) {
                slugs.insert(pid.to_string(), slug.to_string());
            }
        }
    }

    let Some(entries) = memberships.as_array() else {
        return memberships.clone();
    };
    Value::Array(
        entries
            .iter()
            .map(|entry| {
                let slug = entry
                    .get("organization")
                    .and_then(organization_pid)
                    .and_then(|pid| slugs.get(pid))
                    .cloned();
                let mut entry = entry.clone();
                if let (Some(slug), Some(Value::Object(organization))) =
                    (slug, entry.get_mut("organization"))
                {
                    organization
                        .entry("slug".to_string())
                        .or_insert(Value::String(slug));
                }
                entry
            })
            .collect(),
    )
}

/// `org list` rows: one per membership, one per pending invitation.
fn membership_rows(memberships: &Value, selected: Option<&str>, profile: &str) -> Vec<Value> {
    memberships
        .as_array()
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| membership_row(entry, selected, profile))
                .collect()
        })
        .unwrap_or_default()
}

/// One membership or pending invitation. The organization marks the row the
/// profile already points at; a pending invitation carries `invited` where a
/// membership carries `joined`. The profile the command ran with is named on
/// every row, membership or invitation.
fn membership_row(entry: &Value, selected: Option<&str>, profile: &str) -> Option<Value> {
    let organization = entry.get("organization")?;
    let slug = organization.get("slug").and_then(Value::as_str);
    let name = organization
        .get("name")
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty())
        .or(slug)
        .unwrap_or("");
    let role = entry
        .pointer("/role/slug")
        .or_else(|| entry.pointer("/role/name"))
        .and_then(Value::as_str);
    let invited = entry
        .get("invitation")
        .is_some_and(|invitation| !invitation.is_null());

    let mut row = Map::new();
    row.insert(
        "current".to_string(),
        Value::Bool(
            selected.is_some_and(|selected| organization_matches(organization, selected)),
        ),
    );
    if let Some(slug) = slug {
        row.insert("slug".to_string(), Value::String(slug.to_string()));
    }
    row.insert("name".to_string(), Value::String(name.to_string()));
    if let Some(role) = role {
        row.insert("role".to_string(), Value::String(role.to_string()));
    }
    if invited {
        row.insert("invited".to_string(), Value::Bool(true));
    } else if let Some(joined) = entry
        .get("joined_at")
        .or_else(|| organization.get("joined_at"))
        .and_then(Value::as_str)
        .filter(|joined| !joined.is_empty())
    {
        row.insert("joined".to_string(), Value::String(joined.to_string()));
    }
    row.insert("profile".to_string(), Value::String(profile.to_string()));
    Some(Value::Object(row))
}

/// The `org show` fields: what a person asked about. Billing, settings and
/// other internals never appear.
const SUMMARY_FIELDS: [&str; 9] = [
    "name",
    "slug",
    "pid",
    "status",
    "paying_customer",
    "trial_ends_at",
    "created_at",
    "description",
    "max_members",
];

/// The curated `org show` object: only known fields that are actually set, plus
/// the profile the command ran with — a local fact, never the server payload's.
fn organization_summary(organization: &Value, profile: &str) -> Value {
    let mut summary = Map::new();
    for field in SUMMARY_FIELDS {
        let value = if field == "pid" {
            organization_pid(organization).map(|pid| Value::String(pid.to_string()))
        } else {
            organization
                .get(field)
                .filter(|value| !value.is_null())
                .cloned()
        };
        if let Some(value) = value {
            summary.insert(field.to_string(), value);
        }
    }
    summary.insert("profile".to_string(), Value::String(profile.to_string()));
    Value::Object(summary)
}

/// `org members list` rows.
fn member_rows(members: &Value) -> Vec<Value> {
    members
        .as_array()
        .map(|members| members.iter().map(member_row).collect())
        .unwrap_or_default()
}

/// One member: the name the platform stores as two fields, plus the role and
/// the two timestamps.
fn member_row(member: &Value) -> Value {
    let name = match (
        member
            .get("first_name")
            .and_then(Value::as_str)
            .unwrap_or_default(),
        member
            .get("last_name")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    ) {
        ("", "") => None,
        (first, "") => Some(first.to_string()),
        ("", last) => Some(last.to_string()),
        (first, last) => Some(format!("{first} {last}")),
    };

    let mut row = Map::new();
    if let Some(name) = name {
        row.insert("name".to_string(), Value::String(name));
    }
    for (column, field) in [
        ("email", "email"),
        ("role", "role"),
        ("joined", "joined_at"),
        ("last_active", "last_active_at"),
    ] {
        if let Some(value) = member
            .get(field)
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
        {
            row.insert(column.to_string(), Value::String(value.to_string()));
        }
    }
    Value::Object(row)
}

/// `org activity list` rows.
fn activity_rows(logs: &Value) -> Vec<Value> {
    logs.as_array()
        .map(|logs| logs.iter().map(activity_row).collect())
        .unwrap_or_default()
}

/// One activity entry: who did it, what they did, to what, and when.
fn activity_row(log: &Value) -> Value {
    let mut row = Map::new();
    for (column, field) in [
        ("actor", "user_name"),
        ("action", "action"),
        ("subject", "subject_type"),
        ("when", "created_at"),
    ] {
        if let Some(value) = log
            .get(field)
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
        {
            row.insert(column.to_string(), Value::String(value.to_string()));
        }
    }
    Value::Object(row)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    use crate::config::builtin_profile;

    use super::super::{is_org_pid, org_reference, resolve_org_pid};

    fn global(org: Option<&str>) -> GlobalArgs {
        GlobalArgs {
            profile: None,
            base_url: None,
            org: org.map(str::to_owned),
            format: None,
            json: false,
            no_color: false,
            timeout: 5,
            poll_interval: None,
            yes: false,
            dry_run: false,
            quiet: false,
            verbose: false,
            debug: false,
            help: None,
            version: None,
        }
    }

    fn profile(org: Option<&str>) -> crate::config::Profile {
        let mut profile = builtin_profile("prod").expect("prod is a built-in");
        profile.org = org.map(str::to_owned);
        profile
    }

    /// Serve the canned responses in order on a loopback port, recording each
    /// request head it received. Every org request is a bodyless GET, so the
    /// head is the whole request.
    async fn serve(responses: Vec<String>) -> (String, std::sync::mpsc::Receiver<String>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (sink, captured) = std::sync::mpsc::channel();
        tokio::spawn(async move {
            for body in responses {
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                let mut chunk = [0u8; 1024];
                loop {
                    let read = socket.read(&mut chunk).await.unwrap();
                    if read == 0 {
                        break;
                    }
                    request.extend_from_slice(&chunk[..read]);
                    if request.windows(4).any(|window| window == b"\r\n\r\n") {
                        break;
                    }
                }
                sink.send(String::from_utf8_lossy(&request).to_string())
                    .unwrap();
                socket.write_all(response.as_bytes()).await.unwrap();
                socket.shutdown().await.ok();
            }
        });
        (format!("http://{addr}"), captured)
    }

    /// The request line of the one request a client made.
    fn request_line(captured: &std::sync::mpsc::Receiver<String>) -> String {
        captured
            .try_iter()
            .next()
            .expect("the client sent a request")
            .lines()
            .next()
            .unwrap_or_default()
            .to_string()
    }

    fn envelope(data: Value) -> String {
        json!({
            "status": "success",
            "data": data,
            "message": null,
            "status_code": 200,
        })
        .to_string()
    }

    #[test]
    fn the_organization_reference_prefers_the_commands_own_argument() {
        // Positional beats the flag; the flag beats the profile; empty is unset.
        assert_eq!(
            org_reference(
                Some("acme"),
                &global(Some("flag")),
                &profile(Some("stored"))
            ),
            Some("acme")
        );
        assert_eq!(
            org_reference(None, &global(Some("flag")), &profile(Some("stored"))),
            Some("flag")
        );
        assert_eq!(
            org_reference(None, &global(None), &profile(Some("stored"))),
            Some("stored")
        );
        assert_eq!(
            org_reference(Some(""), &global(Some("")), &profile(Some("stored"))),
            Some("stored")
        );
        assert_eq!(
            org_reference(Some(""), &global(Some("")), &profile(Some(""))),
            None
        );
        assert_eq!(org_reference(None, &global(None), &profile(None)), None);
    }

    #[test]
    fn organization_pids_are_told_apart_from_slugs() {
        for pid in [
            "org_0123456789abcdef0123456789abcdef",
            "org_ABCDEF",
            "org_0",
        ] {
            assert!(is_org_pid(pid), "{pid} is a pid");
        }
        for slug in ["acme", "org_", "org_zz", "organization_1", "ORG_1", "org-1"] {
            assert!(!is_org_pid(slug), "{slug} is not a pid");
        }
    }

    #[test]
    fn organization_lookup_matches_pid_or_slug_exactly() {
        let organizations = json!([
            {"pid": "org_aa", "slug": "acme", "name": "Acme"},
            {"id": "org_bb", "slug": "acme-labs", "name": "Acme Labs"},
        ]);

        assert_eq!(
            organization_pid(find_organization(&organizations, "org_aa").unwrap()),
            Some("org_aa")
        );
        assert_eq!(
            organization_pid(find_organization(&organizations, "acme-labs").unwrap()),
            Some("org_bb")
        );
        // Never a prefix or a substring.
        for near in ["acm", "acme-", "acme-lab", "org_a", "ORG_AA", ""] {
            assert!(
                find_organization(&organizations, near).is_none(),
                "{near:?} must not match"
            );
        }
    }

    #[tokio::test]
    async fn a_slug_resolves_through_the_organization_lookup() {
        let (base, captured) = serve(vec![envelope(json!([
            {"pid": "org_a1", "slug": "acme", "name": "Acme"},
        ]))])
        .await;
        let client = ApiClient::with_token(&base, "test-token".to_string(), 5);

        assert_eq!(resolve_org_pid(&client, "acme").await.unwrap(), "org_a1");
        assert_eq!(request_line(&captured), "GET /organizations HTTP/1.1");
    }

    /// `org use` echoes the organization's own name, and that name is the
    /// server's text: an escape, a BEL or a right-to-left override in it must
    /// never reach the confirmation line. This drives the same listing fetch the
    /// command does, then the exact line it prints.
    #[tokio::test]
    async fn the_use_confirmation_drops_terminal_control_sequences() {
        let (base, captured) = serve(vec![envelope(json!([
            {"pid": "org_a1", "slug": "acme", "name": "Acme\u{1b}[2J\u{7}\u{202e}evil"},
        ]))])
        .await;
        let client = ApiClient::with_token(&base, "test-token".to_string(), 5);

        let organizations = client.get_unscoped("/organizations", &[]).await.unwrap();
        let organization = find_organization(&organizations, "acme").unwrap();
        let pid = organization_pid(organization).unwrap().to_string();

        let line = use_confirmation(organization, "acme", &pid);
        assert!(!line.contains('\u{1b}'), "{line:?}");
        assert!(!line.contains('\u{7}'), "{line:?}");
        assert!(!line.contains('\u{202e}'), "{line:?}");
        // The text around the dropped characters is untouched.
        assert_eq!(line, "Now using Acme[2Jevil (org_a1)");
        assert_eq!(request_line(&captured), "GET /organizations HTTP/1.1");
    }

    #[tokio::test]
    async fn a_pid_is_used_without_a_lookup() {
        // Nothing listens on port 1: reaching the network at all would fail.
        let client = ApiClient::with_token("http://127.0.0.1:1", "test-token".to_string(), 5);
        assert_eq!(
            resolve_org_pid(&client, "org_c0ffee").await.unwrap(),
            "org_c0ffee"
        );
    }

    #[tokio::test]
    async fn an_unknown_slug_is_a_usage_error_naming_the_listing() {
        let (base, _captured) = serve(vec![envelope(json!([]))]).await;
        let client = ApiClient::with_token(&base, "test-token".to_string(), 5);

        let err = resolve_org_pid(&client, "nope").await.unwrap_err();
        assert!(matches!(err, Error::Usage(_)), "{err:?}");
        assert_eq!(err.exit_code(), 2);
        assert!(err.to_string().contains("selfhost org list"), "{err}");
    }

    #[tokio::test]
    async fn the_listing_reads_the_membership_endpoint_without_a_scope() {
        let (base, captured) = serve(vec![
            envelope(json!([
                {
                    "organization": {"id": "org_a1", "name": "Acme", "joined_at": "2026-01-02T03:04:05Z"},
                    "role": {"slug": "owner"},
                },
            ])),
            envelope(json!([{"pid": "org_a1", "slug": "acme", "name": "Acme"}])),
        ])
        .await;
        // Even with an org injected, the membership view stays unscoped: the
        // endpoint is the caller's identity, not an organization's.
        let client = ApiClient::with_token(&base, "test-token".to_string(), 5)
            .with_org(Some("org_stale".to_string()));

        let rows = membership_listing(&client, Some("org_a1"), "work").await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["current"], true);
        assert_eq!(rows[0]["joined"], "2026-01-02T03:04:05Z");
        assert_eq!(rows[0]["profile"], "work");
        assert_eq!(request_line(&captured), "GET /users/memberships HTTP/1.1");
    }

    #[tokio::test]
    async fn org_list_marks_the_current_organization_when_the_profile_holds_a_slug() {
        let (base, captured) = serve(vec![
            envelope(json!([
                {
                    "organization": {"id": "org_a1", "name": "Acme"},
                    "role": {"slug": "owner"},
                },
                {
                    "organization": {"id": "org_b2", "name": "Beta"},
                    "role": {"slug": "member"},
                },
            ])),
            envelope(json!([
                {"pid": "org_a1", "slug": "acme", "name": "Acme"},
                {"pid": "org_b2", "slug": "beta", "name": "Beta"},
            ])),
        ])
        .await;
        let client = ApiClient::with_token(&base, "test-token".to_string(), 5);

        // The profile stores the slug, the payloads carry pids: the merged slug
        // is what marks the row.
        let rows = membership_listing(&client, Some("acme"), "work").await.unwrap();
        assert_eq!(rows[0]["current"], true);
        assert_eq!(rows[1]["current"], false);
        assert_eq!(rows[0]["profile"], "work");

        let requests: Vec<String> = captured
            .try_iter()
            .map(|request| request.lines().next().unwrap_or_default().to_string())
            .collect();
        assert_eq!(
            requests,
            [
                "GET /users/memberships HTTP/1.1",
                "GET /organizations HTTP/1.1",
            ]
        );
    }

    #[tokio::test]
    async fn org_list_rows_carry_the_slug_merged_from_the_organization_lookup() {
        let (base, _captured) = serve(vec![
            envelope(json!([
                {
                    "organization": {"id": "org_a1", "name": "Acme", "joined_at": "2026-01-02T03:04:05Z"},
                    "role": {"slug": "owner"},
                },
                {
                    "organization": {"id": "org_c3", "name": "Gamma"},
                    "role": {"slug": "member"},
                    "invitation": {"status": "pending", "invitation_id": "inv_1"},
                },
            ])),
            // The pending invitation's organization is absent from the lookup.
            envelope(json!([{"pid": "org_a1", "slug": "acme", "name": "Acme"}])),
        ])
        .await;
        let client = ApiClient::with_token(&base, "test-token".to_string(), 5);

        let rows = membership_listing(&client, None, "work").await.unwrap();
        assert_eq!(rows[0]["slug"], "acme");
        assert_eq!(rows[1]["invited"], true);
        assert!(rows[1].get("slug").is_none(), "{:?}", rows[1]);
        assert_eq!(rows[0]["profile"], "work");
        assert_eq!(rows[1]["profile"], "work");
    }

    #[tokio::test]
    async fn the_shown_organization_comes_from_its_own_endpoint() {
        let (base, captured) = serve(vec![envelope(json!({
            "pid": "org_a1",
            "name": "Acme",
            "slug": "acme",
            "status": "active",
            "paying_customer": false,
            "settings": {"payment_method_link_setup": {"order_id": "o1"}},
        }))])
        .await;
        let client = ApiClient::with_token(&base, "test-token".to_string(), 5)
            .with_org(Some("org_a1".to_string()));

        let summary = organization_show(&client, "org_a1", "work").await.unwrap();
        assert_eq!(summary["pid"], "org_a1");
        assert_eq!(summary["paying_customer"], false);
        assert_eq!(summary["profile"], "work");
        assert!(summary.get("settings").is_none(), "{summary:?}");
        assert_eq!(
            request_line(&captured),
            "GET /organizations/org_a1?organization_id=org_a1 HTTP/1.1"
        );
    }

    #[tokio::test]
    async fn members_come_from_the_organization_endpoint() {
        let (base, captured) = serve(vec![envelope(json!([
            {"email": "aziz@example.com", "first_name": "Aziz", "last_name": "Ansari", "role": "owner"},
        ]))])
        .await;
        let client = ApiClient::with_token(&base, "test-token".to_string(), 5)
            .with_org(Some("org_a1".to_string()));

        let rows = member_listing(&client, "org_a1").await.unwrap();
        assert_eq!(rows[0]["name"], "Aziz Ansari");
        assert_eq!(rows[0]["role"], "owner");
        assert_eq!(
            request_line(&captured),
            "GET /organizations/org_a1/members?organization_id=org_a1 HTTP/1.1"
        );
    }

    #[tokio::test]
    async fn activity_is_paged_through_the_organization_endpoint() {
        let (base, captured) = serve(vec![envelope(json!([
            {"action": "instance_created", "user_name": "Aziz", "created_at": "2026-03-04T05:06:07Z"},
        ]))])
        .await;
        let client = ApiClient::with_token(&base, "test-token".to_string(), 5)
            .with_org(Some("org_a1".to_string()));

        let rows = activity_listing(&client, "org_a1", 2, 50).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["action"], "instance_created");
        assert_eq!(
            request_line(&captured),
            "GET /organizations/org_a1/activity_logs?page=2&limit=50&organization_id=org_a1 HTTP/1.1"
        );
    }

    #[test]
    fn membership_rows_mark_the_selected_organization_and_pending_invitations() {
        let memberships = json!([
            {
                "organization": {
                    "id": "org_a1",
                    "name": "Acme",
                    "slug": "acme",
                    "joined_at": "2026-01-02T03:04:05Z",
                },
                "role": {"pid": "role_1", "name": "Owner", "slug": "owner"},
            },
            {
                "organization": {"id": "org_b2", "name": "Beta", "joined_at": "2026-02-03T04:05:06Z"},
                "role": {"pid": "role_2", "name": "Member", "slug": "member"},
            },
            {
                "organization": {"id": "org_c3", "name": "Gamma"},
                "role": {"pid": "role_2", "name": "Member", "slug": "member"},
                "invitation": {"status": "pending", "invitation_id": "inv_1"},
            },
        ]);

        let rows = membership_rows(&memberships, Some("acme"), "work");
        assert_eq!(rows.len(), 3);

        let acme = &rows[0];
        assert_eq!(acme["current"], true);
        assert_eq!(acme["slug"], "acme");
        assert_eq!(acme["name"], "Acme");
        assert_eq!(acme["role"], "owner");
        assert_eq!(acme["profile"], "work");
        assert!(acme.get("joined").is_some(), "{acme:?}");

        // Selected by pid, and not the selected one.
        let beta = &rows[1];
        assert_eq!(beta["current"], false);
        assert_eq!(beta["name"], "Beta");
        assert_eq!(beta["joined"], "2026-02-03T04:05:06Z");
        assert_eq!(beta["profile"], "work");
        assert!(beta.get("invited").is_none(), "{beta:?}");

        let invited = &rows[2];
        assert_eq!(invited["current"], false);
        assert_eq!(invited["invited"], true);
        assert_eq!(invited["profile"], "work");
        assert!(invited.get("joined").is_none(), "{invited:?}");
        // No slug in the payload, no null slug in the row.
        assert!(invited.get("slug").is_none(), "{invited:?}");

        // The profile may still hold a slug or a pid; a pid marks the row too.
        let by_pid = membership_rows(&memberships, Some("org_b2"), "work");
        assert_eq!(by_pid[1]["current"], true);
        assert_eq!(by_pid[0]["current"], false);

        // Nothing selected marks nothing.
        let unselected = membership_rows(&memberships, None, "work");
        assert!(unselected.iter().all(|row| row["current"] == false));
    }

    #[test]
    fn a_row_without_optional_fields_still_names_the_profile_and_never_nulls() {
        // A bare membership: no slug, no role, no timestamp, and the
        // invitation's organization absent from the lookup.
        let memberships = json!([
            {"organization": {"id": "org_a1", "name": "Acme"}},
            {
                "organization": {"id": "org_c3", "name": "Gamma"},
                "invitation": {"status": "pending", "invitation_id": "inv_1"},
            },
        ]);

        let rows = membership_rows(&memberships, None, "personal");
        assert_eq!(rows.len(), 2);
        for row in &rows {
            assert_eq!(row["profile"], "personal");
            // The value is always the resolved name, and nothing is ever null.
            assert!(
                row.as_object().is_some_and(|row| row.values().all(|value| !value.is_null())),
                "null in {row:?}"
            );
            assert!(row.get("role").is_none(), "{row:?}");
            assert!(row.get("joined").is_none(), "{row:?}");
        }

        // A profile name flows through unchanged, whatever it is.
        let renamed = membership_rows(&memberships, None, "Team EU (prod)");
        assert_eq!(renamed[0]["profile"], "Team EU (prod)");
    }

    #[test]
    fn the_shown_organization_is_curated() {
        let organization = json!({
            "pid": "org_a1",
            "name": "Acme",
            "slug": "acme",
            "status": "active",
            "paying_customer": true,
            "trial_ends_at": null,
            "created_at": "2025-12-01T00:00:00Z",
            "description": null,
            "max_members": 100,
            "settings": {"payment_method_link_setup": {"order_id": "o1"}},
            "billing_email": "billing@acme.test",
            "creator_id": "user_1",
        });

        let summary = organization_summary(&organization, "work");
        assert_eq!(
            summary,
            json!({
                "name": "Acme",
                "slug": "acme",
                "pid": "org_a1",
                "status": "active",
                "paying_customer": true,
                "created_at": "2025-12-01T00:00:00Z",
                "max_members": 100,
                "profile": "work",
            })
        );
        // Absent fields are absent, not null.
        assert!(summary.get("trial_ends_at").is_none(), "{summary:?}");
        assert!(summary.get("description").is_none(), "{summary:?}");
        // Internals never leak.
        for internal in ["settings", "billing_email", "creator_id"] {
            assert!(
                summary.get(internal).is_none(),
                "{internal} leaked: {summary:?}"
            );
        }
    }

    #[test]
    fn member_rows_join_the_names_and_time_fields() {
        let members = json!([
            {
                "id": "user_1",
                "email": "aziz@example.com",
                "first_name": "Aziz",
                "last_name": "Ansari",
                "role": "owner",
                "joined_at": "2026-01-02T03:04:05Z",
                "last_active_at": "2026-03-04T05:06:07Z",
            },
            {
                "id": "user_2",
                "email": "dana@example.com",
                "first_name": "",
                "last_name": "",
                "role": "member",
                "joined_at": null,
                "last_active_at": null,
            },
        ]);

        let rows = member_rows(&members);
        assert_eq!(
            rows[0],
            json!({
                "name": "Aziz Ansari",
                "email": "aziz@example.com",
                "role": "owner",
                "joined": "2026-01-02T03:04:05Z",
                "last_active": "2026-03-04T05:06:07Z",
            })
        );
        assert_eq!(
            rows[1],
            json!({"email": "dana@example.com", "role": "member"})
        );
    }

    #[test]
    fn activity_rows_keep_the_actor_action_subject_and_time() {
        let logs = json!([
            {
                "pid": "log_1",
                "action": "instance_created",
                "subject_type": "CloudInstance",
                "subject_id": "awsinst_1",
                "user_name": "Aziz",
                "created_at": "2026-03-04T05:06:07Z",
                "ip_address": "203.0.113.7",
                "metadata": {"size": "100gb"},
            },
            {
                "pid": "log_2",
                "action": "instance_deleted",
                "subject_type": null,
                "user_name": null,
                "created_at": "2026-03-05T06:07:08Z",
            },
        ]);

        let rows = activity_rows(&logs);
        assert_eq!(
            rows[0],
            json!({
                "actor": "Aziz",
                "action": "instance_created",
                "subject": "CloudInstance",
                "when": "2026-03-04T05:06:07Z",
            })
        );
        assert_eq!(
            rows[1],
            json!({
                "action": "instance_deleted",
                "when": "2026-03-05T06:07:08Z",
            })
        );
    }
}
