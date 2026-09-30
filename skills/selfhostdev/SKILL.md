---
name: selfhostdev
description: >-
  Use this skill whenever a task touches selfhost.dev from the terminal: signing
  in, choosing a profile or organization, managing organizations, members and
  invitations, working with managed databases (Postgres, MySQL, Mongo, Redis,
  ClickHouse, OpenSearch), servers, projects, deployments, domains, networking or
  billing, or calling any platform endpoint directly. Trigger it when the user
  mentions selfhost, selfhost.dev or api.selfhost.dev, and also when they only
  describe the goal without naming the tool — "spin up a Postgres database",
  "deploy this repo", "why is my staging database down", "what is on my
  invoice". Not for developing the CLI's own source code.
license: Apache-2.0
compatibility: >-
  Requires the selfhost CLI on PATH and network access to api.selfhost.dev.
  Commands act on behalf of a signed-in profile.
metadata:
  version: "0.1.0"
---

# SelfHost CLI

`selfhost` manages a selfhost.dev account from the terminal — organizations,
managed databases, servers, projects, deployments and billing — and can call any
platform endpoint the typed commands do not cover yet.

## Setup

Build the binary from the project's repository
(github.com/selfhost-dev/selfhost-cli, Rust 1.88+), put it on `PATH`, then sign
in once per profile. The CLI stores the credentials and renews them over HTTPS
without a browser, so a machine signed in once keeps working unattended.

```sh
cargo build --release                                          # target/release/selfhost
install -m 755 target/release/selfhost ~/.local/bin/selfhost   # any directory on PATH

selfhost auth login            # imports an existing MCP credential pair when there is one, otherwise opens the console's sign-in page and waits for it
selfhost auth login --no-browser  # prints the URL and an ssh -L command; the redirect URL can be pasted back
selfhost auth status           # checklist: store, profile, endpoints, credentials, organization
```

Credentials live in the selected profile inside `~/.selfhost/config.json`
(directory 0700, file 0600). Profiles are `profile list|show|add|remove|use|set`;
choose one with `-p/--profile` or `SELFHOSTDEV_PROFILE` — an unknown name is a
usage error, never created implicitly. Save a default organization with
`selfhost profile set <profile> org <slug>`.

Non-interactive runs need credentials that already exist: there is no token flag
and no device-code flow. Either the profile is signed in, or both
`FIREBASE_API_KEY` and `FIREBASE_REFRESH_TOKEN` are set (the pair the MCP server
uses); the environment pair wins over the profile's stored pair. `auth login`
is not always a browser hop: when the profile holds no credentials and
`~/.selfhost/credentials.json` exists — the file the SelfHost MCP server
writes — the CLI offers to import that pair instead, prints "Found sign-in
credentials from the SelfHost MCP server for profile '<name>'. Import them
into this profile? [Y/n]", and answers yes to a bare Enter. The offer needs
someone to ask: without a terminal on stdin and without `--yes` the import is
skipped entirely and the command goes straight to the browser hop, so a
non-interactive `auth login` still ends at a page nobody can open. On a
terminal, agreeing (or passing `--yes`) saves the pair into the profile and the
command ends right there, with no browser. It falls back to the browser flow
when the import is declined, when the file is missing or unreadable, when the
pair fails verification, and when the pair verifies but cannot be written into
the profile — the last two print a warning naming the file or the save failure
and then continue with browser sign-in. The file itself is only ever read, so
the MCP server keeps working.

## Command map

| Groups | Live today |
| --- | --- |
| `auth`, `profile`, `org`, `api` | working |
| `tui`, `tree`, `completion`, `help` | working, local only, no network |

Everything else is a declared-but-unbuilt group: each verb answers
`not implemented yet: <group> <verb>` (exit 1). The work behind those verbs is
not missing — it is reachable today through `selfhost api`, and each group has a
reference that maps its future verbs to the real endpoints:

| Group | Raw API reference |
| --- | --- |
| `postgres` | [references/postgresql.md](references/postgresql.md) |
| `mysql` | [references/mysql.md](references/mysql.md) |
| `mongo` | [references/mongodb.md](references/mongodb.md) |
| `redis` | [references/redis.md](references/redis.md) |
| `clickhouse` | [references/clickhouse.md](references/clickhouse.md) |
| `opensearch` | [references/opensearch.md](references/opensearch.md) |
| `project` | [references/projects.md](references/projects.md) |
| `catalog` | [references/catalog.md](references/catalog.md) |
| `deploy` | [references/deploys.md](references/deploys.md) |
| `github` | [references/github.md](references/github.md) |
| `domain` | [references/domains.md](references/domains.md) |
| `billing` | [references/billing.md](references/billing.md) |
| `cloud` | [references/cloud.md](references/cloud.md) |
| `network` | [references/network.md](references/network.md) |
| `ssh-key` | [references/ssh-keys.md](references/ssh-keys.md) |
| `alert` | [references/alerts.md](references/alerts.md) |
| `scaling` | [references/scaling.md](references/scaling.md) |
| `webhook` | [references/webhooks.md](references/webhooks.md) |
| `config` | none — local settings, no platform work behind it |

Load the domain's reference before composing a call. Each one lists the
endpoints with their required parameters, the polling rules for async work, and
the traps (several creates are billable, nested bodies cannot be built with
`-f`, some verbs have no endpoint yet), verified against the backend source.
When a typed command for the group lands later, it replaces the raw calls.

## Core workflows

1. **Orient before acting.** `selfhost auth status` reports the profile, the
   resolved organization and whether the credentials work; `selfhost org list`
   lists the memberships. Everything else needs a subcommand.
2. **Find the command.** `selfhost tree` prints the whole tree; `selfhost help
   <path...>` (e.g. `selfhost help org members`) prints one group's page. Check
   the Command map above before promising a group works: a stubbed verb answers
   `not implemented yet: <group> <verb>` and exits 1, so fall back to `api`.
3. **Make a change.** Read the object first (`org show`, `org members list`, …),
   then run the verb. `org delete` wants the organization's name typed back;
   `org members remove` and `org invites revoke` want `y`. From a script pass
   `-y/--yes`, because without a terminal the command stops instead of
   prompting. Verify with a follow-up read — the change's own output is only a
   receipt, and a declined prompt changes nothing.
4. **Reach an endpoint with no command.** Load the group's reference from the
   Command map, then call the endpoint it documents — `selfhost api
   /organizations/{org}/members --org acme`. Use a typed command instead
   whenever one exists, because it validates input, resolves the organization
   and formats the answer.
5. **Report results.** Pass `-o json` (or `--json`) whenever the answer will be
   parsed; stdout carries data, stderr carries errors. Quote slugs, pids and
   statuses, never tokens.

## Gotchas

- A bare `selfhost` on a terminal opens the interactive TUI and waits for input.
  In scripts and CI always pass a subcommand, or set `SELFHOSTDEV_NO_TUI=1`.
- Most of the command tree is scaffolding: a verb can appear in `--help` and
  still answer `not implemented yet`. Confirm a group is live before promising
  the user it will work, and use that group's raw API reference from the Command
  map to do the work today.
- `api` turns a GET into a POST as soon as you add `-f` or `-F`. Listing
  endpoints need an explicit `-X GET`.
- `{org}` in an `api` endpoint needs a resolved organization (`--org`, or the
  profile's default); without one the command exits with a usage error.
- Exit codes carry meaning: 2 is bad usage, 3 is not signed in, 4 is a billing
  problem, 75 is rate limiting. Do not report 3 as a network failure.
- Destructive and money-affecting verbs ask for confirmation; without a terminal
  they refuse unless `--yes` is passed. `--dry-run` is not a safety net here: no
  command previews anything yet (see Working rules), so read before and after
  instead.
- Output is a table on a terminal and JSON when piped. Pass `-o json` when you
  intend to parse the result.
- `auth token` and raw `api` responses can contain live credentials. Never echo
  them into logs, files or the conversation.

## Working rules

- **Profile**: `-p/--profile`, else `SELFHOSTDEV_PROFILE`, else the store's
  default. An unknown name is `usage error: unknown profile 'x'; create it
  first: selfhost profile add x` (exit 2).
- **Organization**: the command's own `[ORG]` argument first, then `--org` /
  `SELFHOSTDEV_ORG`, then the profile's saved org; an empty value counts as
  unset. `org list`, `org use` and `org create` are not org-scoped and ignore
  `--org`. Nothing selected: `usage error: no organization selected; run selfhost
  org use <slug>` (exit 2), raised before credentials are read. A name that is
  not one of your memberships: `usage error: unknown organization '<ref>'; run
  selfhost org list` (exit 2).
- **Credentials**: the environment pair `FIREBASE_API_KEY` + `FIREBASE_REFRESH_TOKEN`
  when both are set, otherwise the profile's stored pair; the environment wins.
  Neither: `not authenticated: profile '<name>' has no Firebase credentials; run
  selfhost auth login` (exit 3). `auth token` prints a live credential — keep it
  and `~/.selfhost/config.json` out of logs, files and answers.
- **Exit codes**: 0 success · 1 runtime or API error, including every `not
  implemented yet` · 2 usage (bad flags, unknown profile or organization, missing
  confirmation, refused `--dry-run`) · 3 not authenticated · 4 billing required
  (API 402) · 75 rate limited (429 after `Retry-After` and retries).
- **Confirmations**: only destructive verbs ask. `--yes` answers up front; with no
  terminal and no `--yes` the command stops with `usage error: <verb> needs
  confirmation; pass --yes to run it non-interactively` (exit 2) before any
  credential is read. Declining a yes/no prompt (`org members remove`, `org
  invites revoke`) exits 1 with `not confirmed; nothing was changed`. `org
  delete` instead asks you to type the organization's name back and exits 1
  with `not confirmed: the name did not match; nothing was deleted` when the
  typed text differs.
- **`--dry-run`** is not a simulation yet: every command refuses it before its
  handler runs (`dry runs are not supported for … yet; nothing was sent`, exit
  2), so nothing is sent, written or read and no store file appears. The message
  names the subject: `organization changes` for `org`, `api calls` for `api`,
  `this command` for everything else — `profile`, `auth`, `tui`, `tree`,
  `completion`, the stub groups, a bare `selfhost --dry-run`.
- **Output**: table on a terminal, JSON when piped; `-o table|json|yaml`, `--json`
  equals `-o json`. `-q/--quiet` does not silence output: it is read in exactly
  one place, to drop `auth token`'s "expires shortly" warning on stderr. Every
  other command ignores it, so `selfhost api … -q` still prints its data on
  stdout. On `api`, `--silent` is the flag that prints no body at all.

## Raw API access

`selfhost api <path>` calls any platform endpoint directly with the selected
profile's sign-in and organization handling. Use it for the stub groups and for
endpoints with no typed command; prefer a typed command when the group is live.

- A parameter flips the call to POST unless `-X` names another method: `-f
  key=value` plain, `-F key=value` typed (a value starting with `@` reads a file,
  `@-` stdin). With `-X GET` the same parameters go on the query string —
  listing endpoints need that.
- The path may omit the leading slash. `{org}` becomes the resolved organization
  pid, and without one the command stops with the `org use` hint. A full URL,
  whitespace or `..` segments are usage errors.
- `-H 'Name: value'` adds a header; `authorization`, `accept`, `content-type` and
  `content-length` are managed by the CLI and rejected. `-i` shows status and
  headers, `--silent` prints nothing (exit code only). Failures keep the API's
  message and the usual exit codes.

Load [references/api.md](references/api.md) when you need endpoint patterns,
envelope shapes or worked examples.

## Troubleshooting

- `not authenticated: profile 'default' has no Firebase credentials; run selfhost
  auth login` (exit 3) — the selected profile has no stored credentials and the
  environment pair is missing or half-set. Run `selfhost auth login` (add `-p
  <name>` for another profile) or set both `FIREBASE_API_KEY` and
  `FIREBASE_REFRESH_TOKEN`.
- `usage error: no organization selected; run selfhost org use <slug>` (exit 2) —
  nothing selected an organization. Run `selfhost org use <slug>` to save a
  default, or pass `--org <slug>` for one command.
- `usage error: unknown organization '<ref>'; run selfhost org list` (exit 2) — a
  slug typo or an organization you are not a member of. Pids (`org_…`) are used
  as-is, so a stale pid is only caught by the API.
- `not implemented yet: <group> <verb>` (exit 1) — the group is a stub, not a
  permission or connectivity problem. Reach the endpoint with `api`; never tell
  the user the typed command exists.
- `billing required: <message>` (exit 4) — the API answered 402, so the account
  behind the profile cannot pay for the request; the message names the cause.
- `rate limited: <message>` (exit 75) — a 429 survived `Retry-After` and the
  built-in retries. Wait before retrying; do not retry in a tight loop.
- `usage error: dry runs are not supported for … yet; nothing was sent` (exit 2)
  — `--dry-run` was passed and nothing simulates it yet (see Working rules, and
  commands.md for the per-subject wording). Nothing was sent; rerun without the
  flag.
- In a script, a bare `selfhost` prints the help on stderr and exits 2 instead of
  waiting — the TUI opens only when stdin and stdout are both terminals. Always
  pass a subcommand, and set `SELFHOSTDEV_NO_TUI=1` for safety.

## Reference files

Load only what the task needs:

| File | Load when |
| --- | --- |
| [references/commands.md](references/commands.md) | running a live group whose flags this skill does not name — it is the full surface |
| [references/api.md](references/api.md) | any raw `api` work: method defaulting, field typing, headers, output, exit codes |
| [postgresql.md](references/postgresql.md), [mysql.md](references/mysql.md), [mongodb.md](references/mongodb.md), [redis.md](references/redis.md), [clickhouse.md](references/clickhouse.md), [opensearch.md](references/opensearch.md) | provisioning or day-2 work on that engine, whether a managed project database or a cloud instance |
| [projects.md](references/projects.md), [services.md](references/services.md), [catalog.md](references/catalog.md) | managed project servers, template services, and choosing a region, instance type, storage or price |
| [deploys.md](references/deploys.md), [github.md](references/github.md), [domains.md](references/domains.md) | shipping a repository, connecting GitHub, attaching custom domains |
| [billing.md](references/billing.md), [cloud.md](references/cloud.md), [network.md](references/network.md), [ssh-keys.md](references/ssh-keys.md), [alerts.md](references/alerts.md), [scaling.md](references/scaling.md), [webhooks.md](references/webhooks.md) | wallet and top-ups, cloud credentials, VPCs and security groups, SSH keys and access, alerting, capacity, outbound webhooks |

A domain reference supersedes the raw-API guesswork: it names the exact
endpoints, the body shapes the API requires, and which operations are billable,
destructive or not implemented yet.
