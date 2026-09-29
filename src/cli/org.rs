//! `org` — organizations, members, invitations, activity log (design §4).
//!
//! `list`, `show`, `use`, `members`, `roles`, `invites`, `activity` and the
//! `create`/`update`/`delete` lifecycle are real; nothing in this group stages.
//!
//! Every org-scoped request goes through the shared rule: a positional
//! organization, then `--org`/`SELFHOSTDEV_ORG`, then the profile's stored org,
//! resolved to a pid (`org_<hex>` used as-is, anything else an exact slug
//! lookup). `org list` never resolves the selected org and `org use` takes its
//! organization only from the command line: the listing has to survive a stale
//! one, and `use` is how a new one is chosen. `org create` is unscoped — the
//! caller owns what they create, so `--org` means nothing to it.
//!
//! `org members add` and `org invites create` are one operation: the API has no
//! add-member call, so the person is invited and becomes a member when they
//! accept. The destructive verbs (`delete`, `members remove`, `invites revoke`)
//! confirm first — `--yes` answers the prompt, and without it they need a
//! terminal (design §6).

use std::collections::HashMap;

use anyhow::anyhow;
use clap::{Args, ValueEnum};
use serde_json::{Map, Value};

use crate::api::ApiClient;
use crate::config::ProfileStore;
use crate::error::{Error, Result};
use crate::output::strip_control_characters;

use super::{
    GlobalArgs, NoArgs, block_on, confirm_typed, confirm_yes_no, find_organization, human_output,
    org_client, organization_matches, organization_pid, print, reject_dry_run, should_confirm,
    unknown_organization, unscoped_client,
};

// `org create <name> [--description <TEXT>]`. The API derives the slug from the
// name and accepts it nowhere, so there is no `--slug` to pass.
#[derive(Debug, Clone, Args)]
pub struct OrgCreateArgs {
    /// Organization name
    pub name: String,

    /// What the organization is for
    #[arg(long)]
    pub description: Option<String>,
}

// `org update [ORG] [--name <TEXT>] [--description <TEXT>]`.
#[derive(Debug, Clone, Args)]
pub struct OrgUpdateArgs {
    /// Organization slug or pid (default: the one you have selected)
    #[arg(value_name = "ORG")]
    pub target: Option<String>,

    /// New name
    #[arg(long)]
    pub name: Option<String>,

    /// New description
    #[arg(long)]
    pub description: Option<String>,
}

// `org delete [ORG]`.
#[derive(Debug, Clone, Args)]
pub struct OrgDeleteArgs {
    /// Organization slug or pid (default: the one you have selected)
    #[arg(value_name = "ORG")]
    pub target: Option<String>,
}

/// A role a person can hold in an organization. The five are the platform's
/// seeded system roles; the API takes the pid, so every `--role` resolves
/// through [`Role::pid`]. The wording matches [`ROLE_CATALOGUE`], which is what
/// `org roles list` prints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Role {
    /// Everything, including billing and deleting the organization
    Owner,
    /// Everything except billing writes and changing who owns the organization
    Admin,
    /// Billing details, payment methods and invoices
    Billing,
    /// Invites people and manages projects
    Manager,
    /// Reads and works with what the organization runs
    Member,
}

impl Role {
    /// The pid the API takes (`role_admin`), and the shape the responses echo.
    pub fn pid(self) -> &'static str {
        match self {
            Self::Owner => "role_owner",
            Self::Admin => "role_admin",
            Self::Billing => "role_billing",
            Self::Manager => "role_manager",
            Self::Member => "role_member",
        }
    }

    /// The slug the API reports and the CLI prints.
    pub fn slug(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Admin => "admin",
            Self::Billing => "billing",
            Self::Manager => "manager",
            Self::Member => "member",
        }
    }
}

/// The five roles the platform seeds, in priority order: the slug, the wire pid,
/// the priority and a short meaning in customer voice. `org roles list` prints
/// this table because no endpoint returns the catalogue — a role added on the
/// server needs a CLI release to appear here.
const ROLE_CATALOGUE: [(&str, &str, u32, &str); 5] = [
    (
        "owner",
        "role_owner",
        100,
        "Everything, including billing and deleting the organization",
    ),
    (
        "admin",
        "role_admin",
        80,
        "Everything except billing writes and changing who owns the organization",
    ),
    (
        "billing",
        "role_billing",
        60,
        "Billing details, payment methods and invoices",
    ),
    (
        "manager",
        "role_manager",
        40,
        "Invites people and manages projects",
    ),
    (
        "member",
        "role_member",
        20,
        "Reads and works with what the organization runs",
    ),
];

// `org members add <email> [--role ROLE]` / `org invites create <email> …`.
#[derive(Debug, Clone, Args)]
pub struct InviteArgs {
    /// Email address of the person to invite
    pub email: String,

    /// Role to grant once they accept (default: member)
    #[arg(long, default_value = "member")]
    pub role: Role,
}

// `org members remove <email>` / `org invites revoke <email>`.
#[derive(Debug, Clone, Args)]
pub struct MemberTargetArgs {
    /// Email address of the person
    pub email: String,
}

// `org members update-role <email> --role ROLE`.
#[derive(Debug, Clone, Args)]
pub struct UpdateRoleArgs {
    /// Email address of the person
    pub email: String,

    /// Role to give them
    #[arg(long)]
    pub role: Role,
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

    /// Invite someone by email; they become a member when they accept
    #[command(name = "add")]
    Add(InviteArgs),

    /// Remove a member; they lose access immediately
    #[command(name = "remove")]
    Remove(MemberTargetArgs),

    /// Change a member's role
    #[command(name = "update-role")]
    UpdateRole(UpdateRoleArgs),
}

impl OrgMembersCommand {
    /// Run one `org members` subcommand against the global options.
    pub fn dispatch(&self, global: &GlobalArgs) -> Result<()> {
        match self {
            Self::List(args) => block_on(members(global, args)),
            Self::Add(args) => block_on(invite(global, args)),
            Self::Remove(args) => block_on(remove_member(global, args)),
            Self::UpdateRole(args) => block_on(update_member_role(global, args)),
        }
    }
}

/// Roles you can grant
#[derive(Debug, Clone, clap::Subcommand)]
pub enum OrgRolesCommand {
    /// List the roles an organization can grant
    ///
    /// The list ships with this CLI: the platform seeds these five roles and has
    /// no endpoint that returns the catalogue, so a role added on the server
    /// needs a newer CLI before it can be granted from here.
    #[command(name = "list")]
    List(NoArgs),
}

impl OrgRolesCommand {
    /// Run one `org roles` subcommand against the global options.
    pub fn dispatch(&self, global: &GlobalArgs) -> Result<()> {
        match self {
            Self::List(_) => roles(global),
        }
    }
}

/// Invitations to join the organization
#[derive(Debug, Clone, clap::Subcommand)]
pub enum OrgInvitesCommand {
    /// List every invitation and its status
    #[command(name = "list")]
    List(OrgTargetArgs),

    /// Invite someone by email; they become a member when they accept
    #[command(name = "create")]
    Create(InviteArgs),

    /// Cancel an invitation that is still waiting
    #[command(name = "revoke")]
    Revoke(MemberTargetArgs),
}

impl OrgInvitesCommand {
    /// Run one `org invites` subcommand against the global options.
    pub fn dispatch(&self, global: &GlobalArgs) -> Result<()> {
        match self {
            Self::List(args) => block_on(invitations(global, args)),
            Self::Create(args) => block_on(invite(global, args)),
            Self::Revoke(args) => block_on(revoke_invitation(global, args)),
        }
    }
}

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

    /// Change an organization's name or description
    #[command(name = "update")]
    Update(OrgUpdateArgs),

    /// Delete an organization and everything in it
    #[command(name = "delete")]
    Delete(OrgDeleteArgs),

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

    /// Invitations to join the organization
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
            Self::Create(args) => block_on(create(global, args)),
            Self::Show(args) => block_on(show(global, args)),
            Self::Update(args) => block_on(update(global, args)),
            Self::Delete(args) => block_on(delete(global, args)),
            Self::Use(args) => block_on(use_organization(global, args)),
            Self::Members(command) => command.dispatch(global),
            Self::Roles(command) => command.dispatch(global),
            Self::Invites(command) => command.dispatch(global),
            Self::Activity(command) => command.dispatch(global),
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
    let (client, pid, _reference) =
        org_client(&mut store, &name, global, args.target.as_deref()).await?;

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
    let (client, pid, _reference) =
        org_client(&mut store, &name, global, args.target.as_deref()).await?;

    let rows = member_listing(&client, &pid).await?;
    print(global, &Value::Array(rows))
}

/// `org invites list [ORG]`: every invitation to one organization, whatever its
/// status — the API returns all of them, not only the pending ones.
async fn invitations(global: &GlobalArgs, args: &OrgTargetArgs) -> Result<()> {
    let mut store = ProfileStore::load()?;
    let name = store.resolved_name(global.profile.as_deref())?;
    let (client, pid, _reference) =
        org_client(&mut store, &name, global, args.target.as_deref()).await?;

    let rows = invitation_listing(&client, &pid).await?;
    print(global, &Value::Array(rows))
}

/// `org roles list`: the roles that ship with the CLI. The platform seeds these
/// five and nothing returns the catalogue, so this needs no credentials and
/// makes no request.
fn roles(global: &GlobalArgs) -> Result<()> {
    print(global, &Value::Array(role_rows()))
}

/// `org activity list [ORG]`: recent entries from one organization's log.
async fn activity(global: &GlobalArgs, args: &OrgActivityArgs) -> Result<()> {
    let mut store = ProfileStore::load()?;
    let name = store.resolved_name(global.profile.as_deref())?;
    let (client, pid, _reference) =
        org_client(&mut store, &name, global, args.target.as_deref()).await?;

    let rows = activity_listing(&client, &pid, args.page, args.limit).await?;
    print(global, &Value::Array(rows))
}

/// `org create <name>`: the caller owns what they create, so the request is
/// unscoped and `--org` means nothing here (like `org list` and `org use`). The
/// platform derives the slug from the name, so the response is the only place
/// the slug the organization got appears.
async fn create(global: &GlobalArgs, args: &OrgCreateArgs) -> Result<()> {
    reject_dry_run(global)?;
    let mut store = ProfileStore::load()?;
    let name = store.resolved_name(global.profile.as_deref())?;
    let client = unscoped_client(&mut store, &name, global).await?;

    let created = create_organization(&client, &args.name, args.description.as_deref()).await?;

    if human_output(global) {
        println!(
            "created organization '{}' ({}, {})",
            stripped(&created, "name"),
            stripped(&created, "slug"),
            strip_control_characters(organization_pid(&created).unwrap_or("")),
        );
    } else {
        print(global, &organization_summary(&created, &name))?;
    }
    Ok(())
}

/// `org update [ORG]`: at least one field to change, then the platform's own
/// record of the result. The update response carries no data, so the summary is
/// a fresh read rather than what we sent.
async fn update(global: &GlobalArgs, args: &OrgUpdateArgs) -> Result<()> {
    reject_dry_run(global)?;
    if args.name.is_none() && args.description.is_none() {
        return Err(Error::Usage(
            "nothing to update: pass --name or --description".to_string(),
        ));
    }

    let mut store = ProfileStore::load()?;
    let name = store.resolved_name(global.profile.as_deref())?;
    let (client, pid, _reference) =
        org_client(&mut store, &name, global, args.target.as_deref()).await?;

    update_organization(
        &client,
        &pid,
        args.name.as_deref(),
        args.description.as_deref(),
    )
    .await?;

    let updated = client.get(&format!("/organizations/{pid}"), &[]).await?;
    if human_output(global) {
        println!(
            "updated organization '{}' ({})",
            stripped(&updated, "name"),
            stripped(&updated, "slug"),
        );
    } else {
        print(global, &organization_summary(&updated, &name))?;
    }
    Ok(())
}

/// `org delete [ORG]`: confirm on the organization's own name before removing
/// it. `--yes` answers the prompt, and the non-interactive gate has already
/// stopped the command when there is neither a terminal nor `--yes`.
async fn delete(global: &GlobalArgs, args: &OrgDeleteArgs) -> Result<()> {
    reject_dry_run(global)?;
    let ask = should_confirm(global, "org delete")?;

    let mut store = ProfileStore::load()?;
    let name = store.resolved_name(global.profile.as_deref())?;
    let (client, pid, _reference) =
        org_client(&mut store, &name, global, args.target.as_deref()).await?;

    // The name to type is the platform's, not what the user typed: the prompt
    // names the organization that is actually about to go. Empty fields are
    // skipped — an empty phrase would confirm on a bare Enter — and the pid is
    // the last resort.
    let organization = client.get(&format!("/organizations/{pid}"), &[]).await?;
    let resolved = organization
        .get("name")
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty())
        .or_else(|| {
            organization
                .get("slug")
                .and_then(Value::as_str)
                .filter(|slug| !slug.is_empty())
        })
        .unwrap_or(pid.as_str());
    if ask
        && !confirm_typed(
            &format!("type {} to confirm: ", strip_control_characters(resolved)),
            resolved.trim(),
        )
        .await
    {
        return Err(Error::Other(anyhow!(
            "not confirmed: the name did not match; nothing was deleted"
        )));
    }

    delete_organization(&client, &pid).await?;

    if human_output(global) {
        println!(
            "deleted organization '{}' ({})",
            stripped(&organization, "name"),
            stripped(&organization, "slug"),
        );
    } else {
        print(global, &deleted_summary(&organization, &pid))?;
    }
    Ok(())
}

/// The `org delete` JSON/YAML object. The name and slug stay byte-exact, the
/// way [`organization_summary`] keeps its fields: JSON escaping already
/// neutralizes control characters, and stripping is only for the human line.
/// A field the platform left out reads as an empty string, never a null.
fn deleted_summary(organization: &Value, pid: &str) -> Value {
    let mut deleted = Map::new();
    for field in ["name", "slug"] {
        deleted.insert(
            field.to_string(),
            Value::String(
                organization
                    .get(field)
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            ),
        );
    }
    deleted.insert("pid".to_string(), Value::String(pid.to_string()));
    let mut value = Map::new();
    value.insert("deleted".to_string(), Value::Object(deleted));
    Value::Object(value)
}

/// `org members add <email>` and `org invites create <email>`: one operation.
/// The API has no add-member call, so the person is invited and the membership
/// appears when they accept.
async fn invite(global: &GlobalArgs, args: &InviteArgs) -> Result<()> {
    reject_dry_run(global)?;
    let mut store = ProfileStore::load()?;
    let name = store.resolved_name(global.profile.as_deref())?;
    let (client, pid, reference) = org_client(&mut store, &name, global, None).await?;

    let invitation = create_invitation(&client, &pid, &args.email, args.role).await?;

    if human_output(global) {
        println!(
            "invited {} to {} as {}; they join when they accept the invitation",
            strip_control_characters(&args.email),
            strip_control_characters(&reference),
            args.role.slug(),
        );
    } else {
        let mut value = Map::new();
        value.insert(
            "invitation".to_string(),
            invitation_object(&invitation, &args.email, args.role, &pid),
        );
        print(global, &Value::Object(value))?;
    }
    Ok(())
}

/// `org members remove <email>`: resolve the email to a user pid, confirm, then
/// remove. Membership is soft-deleted, so a later invitation works.
async fn remove_member(global: &GlobalArgs, args: &MemberTargetArgs) -> Result<()> {
    reject_dry_run(global)?;
    let ask = should_confirm(global, "org members remove")?;

    let mut store = ProfileStore::load()?;
    let name = store.resolved_name(global.profile.as_deref())?;
    let (client, pid, reference) = org_client(&mut store, &name, global, None).await?;

    let members = member_records(&client, &pid).await?;
    let user_pid = member_pid_for(&members, &args.email, &reference)?;

    if ask
        && !confirm_yes_no(&format!(
            "Remove {} from {}?",
            strip_control_characters(&args.email),
            strip_control_characters(&reference)
        ))
        .await
    {
        return Err(Error::Other(anyhow!("not confirmed; nothing was changed")));
    }

    remove_organization_member(&client, &pid, &user_pid).await?;

    if human_output(global) {
        println!(
            "removed {} from {}",
            strip_control_characters(&args.email),
            strip_control_characters(&reference),
        );
    } else {
        let mut removed = Map::new();
        removed.insert("email".to_string(), Value::String(args.email.clone()));
        removed.insert("organization_pid".to_string(), Value::String(pid));
        let mut value = Map::new();
        value.insert("removed".to_string(), Value::Object(removed));
        print(global, &Value::Object(value))?;
    }
    Ok(())
}

/// `org members update-role <email> --role ROLE`: resolve the email to a user
/// pid, then hand the platform the new role's pid.
async fn update_member_role(global: &GlobalArgs, args: &UpdateRoleArgs) -> Result<()> {
    reject_dry_run(global)?;
    let mut store = ProfileStore::load()?;
    let name = store.resolved_name(global.profile.as_deref())?;
    let (client, pid, reference) = org_client(&mut store, &name, global, None).await?;

    let members = member_records(&client, &pid).await?;
    let user_pid = member_pid_for(&members, &args.email, &reference)?;

    let updated = set_member_role(&client, &pid, &user_pid, args.role).await?;
    let role = updated
        .pointer("/role/slug")
        .and_then(Value::as_str)
        .unwrap_or(args.role.slug());

    if human_output(global) {
        println!(
            "gave {} the {} role in {}",
            strip_control_characters(&args.email),
            strip_control_characters(role),
            strip_control_characters(&reference),
        );
    } else {
        let mut member = Map::new();
        member.insert("email".to_string(), Value::String(args.email.clone()));
        member.insert("role".to_string(), Value::String(role.to_string()));
        member.insert("organization_pid".to_string(), Value::String(pid));
        let mut value = Map::new();
        value.insert("member".to_string(), Value::Object(member));
        print(global, &Value::Object(value))?;
    }
    Ok(())
}

/// `org invites revoke <email>`: only a pending invitation can be cancelled, so
/// the email is matched among the pending ones, confirmed, then deleted.
async fn revoke_invitation(global: &GlobalArgs, args: &MemberTargetArgs) -> Result<()> {
    reject_dry_run(global)?;
    let ask = should_confirm(global, "org invites revoke")?;

    let mut store = ProfileStore::load()?;
    let name = store.resolved_name(global.profile.as_deref())?;
    let (client, pid, reference) = org_client(&mut store, &name, global, None).await?;

    let records = invitation_records(&client, &pid).await?;
    let invitation_pid = pending_invitation_pid(&records, &args.email, &reference)?;

    if ask
        && !confirm_yes_no(&format!(
            "Cancel the invitation for {} in {}?",
            strip_control_characters(&args.email),
            strip_control_characters(&reference)
        ))
        .await
    {
        return Err(Error::Other(anyhow!("not confirmed; nothing was changed")));
    }

    cancel_invitation(&client, &pid, &invitation_pid).await?;

    if human_output(global) {
        println!(
            "cancelled the invitation for {} in {}",
            strip_control_characters(&args.email),
            strip_control_characters(&reference),
        );
    } else {
        let mut cancelled = Map::new();
        cancelled.insert("email".to_string(), Value::String(args.email.clone()));
        cancelled.insert("organization_pid".to_string(), Value::String(pid));
        let mut value = Map::new();
        value.insert("cancelled".to_string(), Value::Object(cancelled));
        print(global, &Value::Object(value))?;
    }
    Ok(())
}

/// `POST /organizations/:pid/invitations` — the one way an email gets into an
/// organization. The body is flat, and the organization comes from the injected
/// `organization_id`: the nested route ignores its path parameter when it
/// resolves the organization.
async fn create_invitation(
    client: &ApiClient,
    pid: &str,
    email: &str,
    role: Role,
) -> Result<Value> {
    let mut body = Map::new();
    body.insert("email".to_string(), Value::String(email.to_string()));
    body.insert(
        "role_pid".to_string(),
        Value::String(role.pid().to_string()),
    );
    client
        .post(
            &format!("/organizations/{pid}/invitations"),
            Value::Object(body),
        )
        .await
}

/// `DELETE /organizations/:pid/invitations/:invitation_pid` — cancels one
/// invitation by pid. The route takes the pid, not the token.
async fn cancel_invitation(client: &ApiClient, pid: &str, invitation_pid: &str) -> Result<Value> {
    client
        .delete(
            &format!("/organizations/{pid}/invitations/{invitation_pid}"),
            Value::Object(Map::new()),
        )
        .await
}

/// `POST /organizations` — the one unscoped write, because the caller owns what
/// they create. The platform derives the slug; the response carries it.
async fn create_organization(
    client: &ApiClient,
    name: &str,
    description: Option<&str>,
) -> Result<Value> {
    let mut organization = Map::new();
    organization.insert("name".to_string(), Value::String(name.to_string()));
    if let Some(description) = description {
        organization.insert(
            "description".to_string(),
            Value::String(description.to_string()),
        );
    }
    let mut body = Map::new();
    body.insert("organization".to_string(), Value::Object(organization));
    client
        .post_unscoped("/organizations", Value::Object(body))
        .await
}

/// `PUT /organizations/:pid` — only the fields the caller named, nested the way
/// the controller expects.
async fn update_organization(
    client: &ApiClient,
    pid: &str,
    name: Option<&str>,
    description: Option<&str>,
) -> Result<Value> {
    let mut organization = Map::new();
    if let Some(name) = name {
        organization.insert("name".to_string(), Value::String(name.to_string()));
    }
    if let Some(description) = description {
        organization.insert(
            "description".to_string(),
            Value::String(description.to_string()),
        );
    }
    let mut body = Map::new();
    body.insert("organization".to_string(), Value::Object(organization));
    client
        .put(&format!("/organizations/{pid}"), Value::Object(body))
        .await
}

/// `DELETE /organizations/:pid` — a soft delete the platform performs; the
/// caller must be its owner or an admin.
async fn delete_organization(client: &ApiClient, pid: &str) -> Result<Value> {
    client
        .delete(&format!("/organizations/{pid}"), Value::Object(Map::new()))
        .await
}

/// `DELETE /organizations/:pid/members/:user_pid` — removal is by user pid, so
/// the caller resolves the email first.
async fn remove_organization_member(
    client: &ApiClient,
    pid: &str,
    user_pid: &str,
) -> Result<Value> {
    client
        .delete(
            &format!("/organizations/{pid}/members/{user_pid}"),
            Value::Object(Map::new()),
        )
        .await
}

/// `PATCH /organizations/:pid/members/:user_pid/role` with the new role pid.
async fn set_member_role(
    client: &ApiClient,
    pid: &str,
    user_pid: &str,
    role: Role,
) -> Result<Value> {
    let mut body = Map::new();
    body.insert(
        "role_pid".to_string(),
        Value::String(role.pid().to_string()),
    );
    client
        .patch(
            &format!("/organizations/{pid}/members/{user_pid}/role"),
            Value::Object(body),
        )
        .await
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

/// `GET /organizations/:pid/members`, raw: the member routes need each row's
/// user pid, which the curated listing drops.
async fn member_records(client: &ApiClient, pid: &str) -> Result<Value> {
    client
        .get(&format!("/organizations/{pid}/members"), &[])
        .await
}

/// `GET /organizations/:pid/members`.
async fn member_listing(client: &ApiClient, pid: &str) -> Result<Vec<Value>> {
    Ok(member_rows(&member_records(client, pid).await?))
}

/// `GET /organizations/:pid/invitations`, raw: revoking matches a pending row
/// and needs its pid, and the curated listing never carries one.
async fn invitation_records(client: &ApiClient, pid: &str) -> Result<Value> {
    client
        .get(&format!("/organizations/{pid}/invitations"), &[])
        .await
}

/// `GET /organizations/:pid/invitations`, curated for `org invites list`. The
/// API returns every status, so the wording is never "pending".
async fn invitation_listing(client: &ApiClient, pid: &str) -> Result<Vec<Value>> {
    Ok(invitation_rows(&invitation_records(client, pid).await?))
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
        Value::Bool(selected.is_some_and(|selected| organization_matches(organization, selected))),
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

/// `org roles list` rows: the slug, the wire pid, the priority and the meaning.
/// The table ships with the CLI, so a role added on the server needs a CLI
/// release before it can appear here.
fn role_rows() -> Vec<Value> {
    ROLE_CATALOGUE
        .iter()
        .map(|(slug, pid, priority, summary)| {
            let mut row = Map::new();
            row.insert("role".to_string(), Value::String((*slug).to_string()));
            row.insert("pid".to_string(), Value::String((*pid).to_string()));
            row.insert("priority".to_string(), Value::from(*priority));
            row.insert("summary".to_string(), Value::String((*summary).to_string()));
            Value::Object(row)
        })
        .collect()
}

/// `org invites list` rows.
fn invitation_rows(invitations: &Value) -> Vec<Value> {
    invitations
        .as_array()
        .map(|invitations| invitations.iter().map(invitation_row).collect())
        .unwrap_or_default()
}

/// One invitation: who it is for, the role, where it stands and when it lapses.
/// The serializer's `access_token`, `invite_by_email` and the rest never reach
/// the terminal: only these curated fields do.
fn invitation_row(invitation: &Value) -> Value {
    let mut row = Map::new();
    for (column, field) in [
        ("email", "email"),
        ("role", "role_assigned"),
        ("status", "status"),
        ("created_at", "created_at"),
        ("expires_at", "expires_at"),
    ] {
        if let Some(value) = invitation
            .get(field)
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
        {
            row.insert(column.to_string(), Value::String(value.to_string()));
        }
    }
    Value::Object(row)
}

/// The invitation object `members add` / `invites create` print: the curated
/// fields, with the role the platform assigned falling back to the one asked
/// for.
fn invitation_object(invitation: &Value, email: &str, role: Role, organization_pid: &str) -> Value {
    let mut object = Map::new();
    object.insert(
        "email".to_string(),
        Value::String(
            invitation
                .get("email")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .unwrap_or(email)
                .to_string(),
        ),
    );
    object.insert(
        "role".to_string(),
        Value::String(
            invitation
                .get("role_assigned")
                .and_then(Value::as_str)
                .unwrap_or(role.slug())
                .to_string(),
        ),
    );
    for field in ["status", "expires_at"] {
        if let Some(value) = invitation.get(field).and_then(Value::as_str) {
            object.insert(field.to_string(), Value::String(value.to_string()));
        }
    }
    object.insert(
        "organization_pid".to_string(),
        Value::String(organization_pid.to_string()),
    );
    Value::Object(object)
}

/// Whether a member row carries `email`, matched exactly and case-insensitively.
fn email_matches(member: &Value, email: &str) -> bool {
    member
        .get("email")
        .and_then(Value::as_str)
        .is_some_and(|candidate| candidate.eq_ignore_ascii_case(email))
}

/// Whether `value` is a plain single path segment: non-empty, and made only of
/// characters that cannot traverse out of a URL path or break it (`/`, `.`,
/// `?`, `#`, whitespace). Server-supplied pids are interpolated into request
/// paths, so anything else must count as absent rather than be sent verbatim.
fn path_segment(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

/// The user pid a member row carries — the `id` the member routes take. Anything
/// that is not a plain path segment counts as no pid, so it never reaches a URL.
fn member_pid(member: &Value) -> Option<&str> {
    member
        .get("id")
        .and_then(Value::as_str)
        .filter(|pid| path_segment(pid))
}

/// The user pid of the one member whose email is `email`. No match is a usage
/// error naming the listing; more than one match is a contradiction and fails
/// loud rather than picking one.
fn member_pid_for(members: &Value, email: &str, organization: &str) -> Result<String> {
    let matches: Vec<&Value> = members
        .as_array()
        .map(|members| {
            members
                .iter()
                .filter(|member| email_matches(member, email))
                .collect()
        })
        .unwrap_or_default();

    match matches.as_slice() {
        [] => Err(Error::Usage(format!(
            "no member with email '{}' in {}; run selfhost org members list",
            strip_control_characters(email),
            strip_control_characters(organization),
        ))),
        [member] => member_pid(member).map(str::to_owned).ok_or_else(|| {
            Error::Other(anyhow!(
                "the member with email '{}' in {} has no id",
                strip_control_characters(email),
                strip_control_characters(organization),
            ))
        }),
        _ => Err(Error::Other(anyhow!(
            "more than one member in {} has the email '{}'; nothing was changed",
            strip_control_characters(organization),
            strip_control_characters(email),
        ))),
    }
}

/// The pid of the one pending invitation for `email`. Only pending rows count:
/// the platform refuses to cancel anything else, so an accepted or cancelled
/// invitation reads as "no pending invitation" here too.
fn pending_invitation_pid(invitations: &Value, email: &str, organization: &str) -> Result<String> {
    let pending: Vec<&Value> = invitations
        .as_array()
        .map(|invitations| {
            invitations
                .iter()
                .filter(|invitation| {
                    invitation.get("status").and_then(Value::as_str) == Some("pending")
                        && email_matches(invitation, email)
                })
                .collect()
        })
        .unwrap_or_default();

    match pending.as_slice() {
        [] => Err(Error::Usage(format!(
            "no pending invitation for '{}' in {}; run selfhost org invites list",
            strip_control_characters(email),
            strip_control_characters(organization),
        ))),
        [invitation] => invitation
            .get("pid")
            .and_then(Value::as_str)
            .filter(|pid| path_segment(pid))
            .map(str::to_owned)
            .ok_or_else(|| {
                Error::Other(anyhow!(
                    "the pending invitation for '{}' in {} has no pid",
                    strip_control_characters(email),
                    strip_control_characters(organization),
                ))
            }),
        _ => Err(Error::Other(anyhow!(
            "more than one pending invitation in {} matches '{}'; nothing was changed",
            strip_control_characters(organization),
            strip_control_characters(email),
        ))),
    }
}

/// A server string from `object`, stripped of the characters that could drive
/// the terminal. A missing or empty field reads as `""`.
fn stripped(object: &Value, field: &str) -> String {
    strip_control_characters(object.get(field).and_then(Value::as_str).unwrap_or(""))
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
    /// whole request (head and body) it received. A request is recorded before
    /// its response is written, so `try_iter` after an awaited call sees it.
    async fn serve(responses: Vec<String>) -> (String, std::sync::mpsc::Receiver<String>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        /// Whether the buffer holds a complete request: headers plus the body
        /// their `Content-Length` promises.
        fn request_complete(buf: &[u8]) -> bool {
            let Some(end) = buf.windows(4).position(|window| window == b"\r\n\r\n") else {
                return false;
            };
            let head = String::from_utf8_lossy(&buf[..end]).to_ascii_lowercase();
            let length = head
                .lines()
                .find_map(|line| {
                    line.strip_prefix("content-length:")
                        .map(|value| value.trim().parse::<usize>().unwrap_or(0))
                })
                .unwrap_or(0);
            buf.len() >= end + 4 + length
        }

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
                    if request_complete(&request) {
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

    /// The whole request — head and body — the one request a client made.
    fn request_text(captured: &std::sync::mpsc::Receiver<String>) -> String {
        captured
            .try_iter()
            .next()
            .expect("the client sent a request")
    }

    /// The request line of the one request a client made.
    fn request_line(captured: &std::sync::mpsc::Receiver<String>) -> String {
        request_text(captured)
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

        let rows = membership_listing(&client, Some("org_a1"), "work")
            .await
            .unwrap();
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
        let rows = membership_listing(&client, Some("acme"), "work")
            .await
            .unwrap();
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
                row.as_object()
                    .is_some_and(|row| row.values().all(|value| !value.is_null())),
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

    /// The `--role` values and the static table are the same five roles: a slug
    /// or pid that drifts between them would send the API a role it does not
    /// have (or print one it does not offer).
    #[test]
    fn every_role_maps_to_a_catalogue_pid_and_slug() {
        let mut values: Vec<&str> = Role::value_variants()
            .iter()
            .map(|role| role.slug())
            .collect();
        values.sort_unstable();
        let mut catalogue: Vec<&str> = ROLE_CATALOGUE.iter().map(|(slug, _, _, _)| *slug).collect();
        catalogue.sort_unstable();
        assert_eq!(values, catalogue);

        for (role, slug, pid) in [
            (Role::Owner, "owner", "role_owner"),
            (Role::Admin, "admin", "role_admin"),
            (Role::Billing, "billing", "role_billing"),
            (Role::Manager, "manager", "role_manager"),
            (Role::Member, "member", "role_member"),
        ] {
            assert_eq!(role.slug(), slug);
            assert_eq!(role.pid(), pid);
            let entry = ROLE_CATALOGUE
                .iter()
                .find(|(catalogue_slug, _, _, _)| *catalogue_slug == slug)
                .unwrap_or_else(|| panic!("{slug} is missing from the catalogue"));
            assert_eq!(entry.1, pid, "{slug}");
        }
    }

    #[test]
    fn the_role_table_lists_the_five_roles_in_priority_order() {
        let rows = role_rows();
        assert_eq!(rows.len(), 5);

        let pids: Vec<&str> = rows.iter().filter_map(|row| row["pid"].as_str()).collect();
        assert_eq!(
            pids,
            [
                "role_owner",
                "role_admin",
                "role_billing",
                "role_manager",
                "role_member",
            ]
        );
        let priorities: Vec<u64> = rows
            .iter()
            .filter_map(|row| row["priority"].as_u64())
            .collect();
        assert_eq!(priorities, [100, 80, 60, 40, 20]);

        for row in &rows {
            for key in ["role", "pid", "priority", "summary"] {
                assert!(row.get(key).is_some(), "{key} missing from {row:?}");
            }
            assert!(
                row["summary"]
                    .as_str()
                    .is_some_and(|summary| summary.len() > 10),
                "{row:?}"
            );
        }
    }

    /// The invitation serializer hands back the raw invitation token. It is
    /// credential material and must never be printed, in a row or in the object
    /// a successful invite reports.
    #[test]
    fn invitation_output_is_curated_and_never_carries_the_access_token() {
        let invitations = json!([{
            "pid": "inv_1",
            "email": "dana@example.com",
            "status": "pending",
            "role_assigned": "manager",
            "created_at": "2026-01-02T03:04:05Z",
            "expires_at": "2026-01-09T03:04:05Z",
            "access_token": "raw-invitation-token",
            "invite_by_email": true,
            "delivered_at": "2026-01-02T03:05:00Z",
        }]);

        let rows = invitation_rows(&invitations);
        assert_eq!(
            rows[0],
            json!({
                "email": "dana@example.com",
                "role": "manager",
                "status": "pending",
                "created_at": "2026-01-02T03:04:05Z",
                "expires_at": "2026-01-09T03:04:05Z",
            })
        );

        let object =
            invitation_object(&invitations[0], "dana@example.com", Role::Manager, "org_a1");
        assert_eq!(
            object,
            json!({
                "email": "dana@example.com",
                "role": "manager",
                "status": "pending",
                "expires_at": "2026-01-09T03:04:05Z",
                "organization_pid": "org_a1",
            })
        );

        let printed = serde_json::to_string(&json!([rows, object])).unwrap();
        for secret in ["access_token", "raw-invitation-token", "invite_by_email"] {
            assert!(!printed.contains(secret), "{secret} leaked: {printed}");
        }
    }

    /// An empty listing is what the API sends as `data: null`, and it lists as
    /// no rows rather than as an error.
    #[test]
    fn an_empty_invitation_listing_is_no_rows() {
        assert!(invitation_rows(&Value::Null).is_empty());
        assert!(invitation_rows(&json!([])).is_empty());
    }

    /// The invite object falls back to the role that was asked for when the API
    /// echoes none, so the field is never missing.
    #[test]
    fn the_invitation_object_falls_back_to_the_requested_role() {
        let object = invitation_object(&json!({}), "dana@example.com", Role::Billing, "org_a1");
        assert_eq!(object["role"], "billing");
        assert_eq!(object["email"], "dana@example.com");
        assert_eq!(object["organization_pid"], "org_a1");
    }

    /// The JSON/YAML output is byte-exact like every other object this CLI
    /// prints: JSON escaping alone neutralizes control characters, so the
    /// `deleted` object must not strip them out first. Stripping is only for
    /// the human line.
    #[test]
    fn the_deleted_object_keeps_the_platforms_bytes_exactly() {
        let organization = json!({"name": "Acme\u{1b}[2J", "slug": "ac\u{7}me"});
        let value = deleted_summary(&organization, "org_a1");
        assert_eq!(value["deleted"]["name"], "Acme\u{1b}[2J");
        assert_eq!(value["deleted"]["slug"], "ac\u{7}me");
        assert_eq!(value["deleted"]["pid"], "org_a1");

        // A field the platform left out is an empty string, never a null.
        assert_eq!(deleted_summary(&json!({}), "org_a1")["deleted"]["name"], "");
    }

    #[test]
    fn a_member_email_matches_exactly_and_case_insensitively() {
        let members = json!([
            {"id": "user_1", "email": "Dana@Example.com"},
            {"id": "user_2", "email": "aziz@example.com"},
        ]);

        assert_eq!(
            member_pid_for(&members, "dana@example.com", "acme").unwrap(),
            "user_1"
        );
        assert_eq!(
            member_pid_for(&members, "AZIZ@EXAMPLE.COM", "acme").unwrap(),
            "user_2"
        );

        // Never a prefix, a substring or a case-folded no-match.
        for near in ["dana@example.co", "ana@example.com", "dana", ""] {
            let err = member_pid_for(&members, near, "acme").unwrap_err();
            assert!(matches!(err, Error::Usage(_)), "{err:?}");
            assert_eq!(err.exit_code(), 2);
            assert!(err.to_string().contains("no member with email"), "{err}");
            assert!(
                err.to_string().contains("selfhost org members list"),
                "{err}"
            );
        }
    }

    #[test]
    fn a_member_without_a_usable_id_fails_loud() {
        let members = json!([
            {"email": "dana@example.com"},
            {"id": "user_1/../2", "email": "aziz@example.com"},
            {"id": "user_1", "email": "dana@example.com"},
            {"id": "user_2", "email": "dana@example.com"},
        ]);

        let err = member_pid_for(&members, "aziz@example.com", "acme").unwrap_err();
        assert!(matches!(err, Error::Other(_)), "{err:?}");
        assert!(err.to_string().contains("has no id"), "{err}");

        let err = member_pid_for(&members, "dana@example.com", "acme").unwrap_err();
        assert!(matches!(err, Error::Other(_)), "{err:?}");
        assert!(err.to_string().contains("more than one member"), "{err}");

        // The invitation pid reaches a request path the same way, so a
        // traversal-shaped one must fail loud rather than be interpolated.
        for hostile in ["", "inv_1/../2", "../inv_2", "inv_1/inv_2", "inv 1"] {
            let invitations = json!([
                {"pid": hostile, "email": "dana@example.com", "status": "pending"},
            ]);
            let err = pending_invitation_pid(&invitations, "dana@example.com", "acme").unwrap_err();
            assert!(matches!(err, Error::Other(_)), "{hostile:?}: {err:?}");
            assert!(err.to_string().contains("has no pid"), "{hostile:?}: {err}");
        }
    }

    #[test]
    fn only_a_pending_invitation_can_be_cancelled() {
        let invitations = json!([
            {"pid": "inv_1", "email": "dana@example.com", "status": "accepted"},
            {"pid": "inv_2", "email": "dana@example.com", "status": "cancelled"},
            {"pid": "inv_3", "email": "dana@example.com", "status": "pending"},
        ]);

        assert_eq!(
            pending_invitation_pid(&invitations, "DANA@example.com", "acme").unwrap(),
            "inv_3"
        );

        // An accepted invitation has nothing left to cancel.
        let accepted = json!([
            {"pid": "inv_1", "email": "aziz@example.com", "status": "accepted"},
        ]);
        let err = pending_invitation_pid(&accepted, "aziz@example.com", "acme").unwrap_err();
        assert!(matches!(err, Error::Usage(_)), "{err:?}");
        assert_eq!(err.exit_code(), 2);
        assert!(err.to_string().contains("no pending invitation"), "{err}");
        assert!(
            err.to_string().contains("selfhost org invites list"),
            "{err}"
        );

        // No invitations at all reads the same way.
        let err = pending_invitation_pid(&Value::Null, "dana@example.com", "acme").unwrap_err();
        assert!(matches!(err, Error::Usage(_)), "{err:?}");
    }

    #[tokio::test]
    async fn creating_an_organization_posts_one_nested_body_without_a_scope() {
        let (base, captured) = serve(vec![envelope(json!({
            "pid": "org_a1",
            "name": "Acme",
            "slug": "acme",
        }))])
        .await;
        let client = ApiClient::with_token(&base, "test-token".to_string(), 5);

        let created = create_organization(&client, "Acme", Some("Notes"))
            .await
            .unwrap();
        assert_eq!(created["slug"], "acme");

        let request = request_text(&captured);
        assert!(
            request.starts_with("POST /organizations HTTP/1.1"),
            "{request}"
        );
        assert!(request.contains(r#""organization":{"#), "{request}");
        assert!(request.contains(r#""name":"Acme""#), "{request}");
        assert!(request.contains(r#""description":"Notes""#), "{request}");
        // Creating is unscoped: the caller owns what they create.
        assert!(!request.contains("organization_id"), "{request}");
    }

    #[tokio::test]
    async fn updating_and_deleting_an_organization_use_their_own_verbs() {
        let (base, captured) = serve(vec![
            envelope(json!(null)),
            envelope(json!({"pid": "org_a1", "name": "Beta", "slug": "acme"})),
            envelope(json!(null)),
        ])
        .await;
        let client = ApiClient::with_token(&base, "test-token".to_string(), 5)
            .with_org(Some("org_a1".to_string()));

        update_organization(&client, "org_a1", Some("Beta"), None)
            .await
            .unwrap();
        let refreshed = client.get("/organizations/org_a1", &[]).await.unwrap();
        delete_organization(&client, "org_a1").await.unwrap();
        assert_eq!(refreshed["name"], "Beta");

        let requests: Vec<String> = captured.try_iter().collect();
        assert!(
            requests[0].starts_with("PUT /organizations/org_a1 HTTP/1.1"),
            "{}",
            requests[0]
        );
        assert!(
            requests[0].contains(r#""organization":{"name":"Beta"}"#),
            "{}",
            requests[0]
        );
        assert!(
            requests[0].contains(r#""organization_id":"org_a1""#),
            "{}",
            requests[0]
        );
        assert!(
            requests[1].starts_with("GET /organizations/org_a1?organization_id=org_a1 HTTP/1.1"),
            "{}",
            requests[1]
        );
        assert!(
            requests[2].starts_with("DELETE /organizations/org_a1 HTTP/1.1"),
            "{}",
            requests[2]
        );
    }

    #[tokio::test]
    async fn removing_a_member_and_setting_a_role_address_the_user_pid() {
        let (base, captured) = serve(vec![
            envelope(json!({
                "user_pid": "user_1",
                "role": {"pid": "role_admin", "slug": "admin", "name": "Admin"},
            })),
            envelope(json!(null)),
        ])
        .await;
        let client = ApiClient::with_token(&base, "test-token".to_string(), 5)
            .with_org(Some("org_a1".to_string()));

        let updated = set_member_role(&client, "org_a1", "user_1", Role::Admin)
            .await
            .unwrap();
        remove_organization_member(&client, "org_a1", "user_1")
            .await
            .unwrap();
        assert_eq!(updated["role"]["slug"], "admin");

        let requests: Vec<String> = captured.try_iter().collect();
        assert!(
            requests[0].starts_with("PATCH /organizations/org_a1/members/user_1/role HTTP/1.1"),
            "{}",
            requests[0]
        );
        assert!(
            requests[0].contains(r#""role_pid":"role_admin""#),
            "{}",
            requests[0]
        );
        assert!(
            requests[1].starts_with("DELETE /organizations/org_a1/members/user_1 HTTP/1.1"),
            "{}",
            requests[1]
        );
    }

    #[tokio::test]
    async fn an_invitation_is_created_and_cancelled_through_the_nested_routes() {
        let (base, captured) = serve(vec![
            envelope(json!({
                "pid": "inv_1",
                "email": "dana@example.com",
                "status": "pending",
                "role_assigned": "manager",
                "expires_at": "2026-01-09T03:04:05Z",
            })),
            envelope(json!(null)),
        ])
        .await;
        let client = ApiClient::with_token(&base, "test-token".to_string(), 5)
            .with_org(Some("org_a1".to_string()));

        let invitation = create_invitation(&client, "org_a1", "dana@example.com", Role::Manager)
            .await
            .unwrap();
        assert_eq!(invitation["pid"], "inv_1");
        cancel_invitation(&client, "org_a1", "inv_1").await.unwrap();

        let requests: Vec<String> = captured.try_iter().collect();
        assert!(
            requests[0].starts_with("POST /organizations/org_a1/invitations HTTP/1.1"),
            "{}",
            requests[0]
        );
        // The nested route resolves its organization from the body, so the pid
        // has to be there even though the path names it too.
        assert!(
            requests[0].contains(r#""organization_id":"org_a1""#),
            "{}",
            requests[0]
        );
        assert!(
            requests[0].contains(r#""email":"dana@example.com""#),
            "{}",
            requests[0]
        );
        assert!(
            requests[0].contains(r#""role_pid":"role_manager""#),
            "{}",
            requests[0]
        );
        assert!(
            requests[1].starts_with("DELETE /organizations/org_a1/invitations/inv_1 HTTP/1.1"),
            "{}",
            requests[1]
        );
    }
}
