# Command reference

Exhaustive surface of the `selfhost` CLI. Source of truth: `selfhost --help`,
`selfhost <group> <verb> --help` and `selfhost tree` (one command per line).
Every name, flag and default below is copied from that output; where a behavior
is not in help text, the file that implements it is named.

Usage: `selfhost [OPTIONS] <COMMAND> [ARGS]`. Global options are accepted before
or after the subcommand.

## Global options

| Option | Env | Default | Meaning |
| --- | --- | --- | --- |
| `-p, --profile <NAME>` | `SELFHOSTDEV_PROFILE` | the store's `default` profile | Saved profile to use |
| `--base-url <URL>` | `SELFHOSTDEV_BASE_URL` | the profile's, else `https://api.selfhost.dev` | API base URL |
| `--org <SLUG\|PID>` | `SELFHOSTDEV_ORG` | the profile's organization | Organization slug or pid |
| `-o, --format <FMT>` | – | `table` on a TTY, `json` when piped | `table`, `json`, `yaml` |
| `--json` | – | – | Print JSON (same as `--format json`) |
| `--no-color` | – | – | Accepted but currently has no effect; output is identical with or without it |
| `--timeout <SECS>` | – | `30` | Seconds to wait for one API request before giving up |
| `--poll-interval <SECS>` | – | – | Accepted but currently has no effect; no command reads it yet |
| `-y, --yes` | – | – | Answer yes to every confirmation prompt |
| `--dry-run` | – | – | Stop before doing anything: nothing is previewed or sent yet |
| `-q, --quiet` | – | – | Quiets only the access token warning; all other output still prints |
| `-v, --verbose` | – | – | Show API requests as they are made |
| `--debug` | – | – | Show full request details (secrets hidden); nothing logs the response |
| `-h, --help` | – | – | Print help (works on every command) |
| `-V, --version` | – | – | Print version (root command only; not listed on subcommands) |

An empty `--base-url`/`SELFHOSTDEV_BASE_URL` or `--org`/`SELFHOSTDEV_ORG` counts
as unset, not as an empty value (`src/cli/mod.rs`). `-v`/`--debug` print request
detail to stderr and never log the response (`src/api/mod.rs`); `-q` quiets only
the `auth token` warning (`src/cli/auth.rs`).

## auth

Sign in, sign out and inspect the current session. `selfhost auth <COMMAND>`

| Subcommand | Arguments / options | What it does |
| --- | --- | --- |
| `login` | `--no-browser` | Sign in, importing an existing MCP credential pair when the profile has none and there is someone to ask; otherwise open the browser's sign-in page (`--no-browser` prints the sign-in URL instead of opening one) |
| `logout` | – | Sign out of the current profile |
| `status` | – | Show who you are signed in as; a checklist that exits with the first failure's own code (3 when sign-in failed, 4 when billing is due, 75 when rate-limited, 1 for anything else) |
| `token` | – | Print the current access token to stdout; a "never log it" warning goes to stderr unless `-q` |

```sh
selfhost auth login --no-browser     # remote machine: open the printed URL yourself
selfhost auth status --format json   # parseable checklist
```

In CI or another headless environment set `FIREBASE_API_KEY` and
`FIREBASE_REFRESH_TOKEN` (README; `src/auth/mod.rs`) and no login step is needed.

## profile

Manage saved profiles: API and console endpoints, default org.
`selfhost profile <COMMAND>`

| Subcommand | Arguments / options | What it does |
| --- | --- | --- |
| `list` | – | List your profiles |
| `show` | `<NAME>` | Show one profile |
| `add` | `<NAME>`, `--base-url <BASE_URL>` (default: the global `--base-url`), `--console-url <CONSOLE_URL>` | Add a profile |
| `remove` | `<NAME>` | Delete a profile |
| `use` | `<NAME>` | Make a profile the default |
| `set` | `<NAME> <KEY> <VALUE>`, `KEY` ∈ `base_url`, `console_url`, `org`, `provider` | Change one setting in a profile |

Store: `~/.selfhost/config.json`. Seeded profiles: `default` (prod endpoints),
`prod`, `qa`, `local`; an unknown `--profile` is a usage error — profiles are
never created implicitly (`src/config/store.rs`).

`profile add` needs to know where the profile points: pass `--base-url` on the
command or the global one. With neither set, nothing is written (exit 2):

```
usage error: where should this profile point? pass --base-url <url>
```

The name must be new — adding a name that already exists changes nothing (exit 2):

```
usage error: profile '<name>' already exists; see it with: selfhost profile show <name>
```

`profile remove` deletes a profile, with one exception: the store's default
profile cannot be removed, so switch to another one first with the profile `use`
command (exit 2, `src/cli/profile.rs`). Selecting a profile with `-p <name>` does
not protect it — the profile you are currently on is removed like any other.

```
usage error: cannot remove the default profile '<name>'; switch first: selfhost profile use <other>
```

An unknown `<NAME>` is a usage error of its own here, and every `profile`
subcommand prints this wording (exit 2, `src/cli/profile.rs`):

```
usage error: unknown profile '<name>'; see the ones you have: selfhost profile list
```

```sh
selfhost profile add staging --base-url https://api.staging.example.com
selfhost profile set qa console_url https://console.qa.example.com
```

## org

Organizations, members, invitations and activity. `selfhost org <COMMAND>`

| Subcommand | Arguments / options | What it does |
| --- | --- | --- |
| `list` | – | List the organizations you belong to; marks the selected one and folds in pending invitations |
| `create` | `<NAME>`, `--description <DESCRIPTION>` | Create an organization (unscoped: `--org` means nothing) |
| `show` | `[ORG]` | Show details for an organization |
| `update` | `[ORG]`, `--name <NAME>`, `--description <DESCRIPTION>` | Change name or description; at least one is required |
| `delete` | `[ORG]` | Delete an organization and everything in it |
| `use` | `<ORG>` | Choose the organization other commands use by default |
| `members` | see below | People in your organization and their roles |
| `roles` | see below | Roles you can grant |
| `invites` | see below | Invitations to join the organization |
| `activity` | see below | What happened in your organization |

Nested groups:

| Command | Arguments / options | What it does |
| --- | --- | --- |
| `org members list` | `[ORG]` | List the people in an organization |
| `org members add` | `<EMAIL>`, `--role <ROLE>` (default `member`) | Invite by email; they become a member when they accept |
| `org members remove` | `<EMAIL>` | Remove a member; they lose access immediately |
| `org members update-role` | `--role <ROLE>` (required), `<EMAIL>` | Change a member's role |
| `org roles list` | – | List the roles an organization can grant (printed by the CLI, not fetched) |
| `org invites list` | `[ORG]` | List every invitation and its status |
| `org invites create` | `<EMAIL>`, `--role <ROLE>` (default `member`) | Invite by email; same operation as `org members add` |
| `org invites revoke` | `<EMAIL>` | Cancel an invitation that is still waiting |
| `org activity list` | `[ORG]`, `--page <PAGE>` (default `1`), `--limit <LIMIT>` (default `20`, up to 200) | Show recent activity in an organization |

`ROLE` values: `owner`, `admin`, `billing`, `manager`, `member`. `org list` and
`org use` ignore `--org` (the listing must work when the saved organization is
gone, and `org use` is how it is replaced).

```sh
selfhost org show acme --format json
selfhost org members add dana@example.com --role manager
```

## api

Call any platform endpoint directly. The escape hatch for every endpoint with no
live command. `selfhost api [OPTIONS] <ENDPOINT>`

| Option | Meaning |
| --- | --- |
| `<ENDPOINT>` | Endpoint path, with or without a leading slash |
| `-X, --method <METHOD>` | `GET`, `POST`, `PUT`, `PATCH` or `DELETE`; pass `GET` to keep parameters on the query string |
| `-f, --raw-field <KEY=VALUE>` | Plain text parameter (repeatable); adding one turns the request into a POST unless you name another method |
| `-F, --field <KEY=VALUE>` | Parameter typed when possible; a value starting with `@` reads a file or stdin (repeatable); also upgrades to POST |
| `--input <FILE>` | Send a file, or stdin with `-`, as the request body |
| `-H, --header <NAME: VALUE>` | Extra header (repeatable) |
| `-i, --include` | Show the status line and headers with the response |
| `--silent` | Print nothing; the exit code says how it went |

Defaults to GET and switches to POST when fields or a body are present
(`src/cli/api.rs`). `{org}` in the path is replaced with the resolved
organization pid, and needs a resolved organization (`--org` or the profile's).
Full URLs, whitespace/control characters and `..` segments in the endpoint are
usage errors. `-H` cannot set `Authorization`, `Accept`, `Content-Type` or
`Content-Length`. `--dry-run` is refused (see below).

```sh
selfhost api /organizations/{org}/members --org acme
selfhost api -X GET /organizations -f per_page=50
```

## Local groups (no network)

| Command | Arguments / options | What it does |
| --- | --- | --- |
| `selfhost tui` | `--view <VIEW>` (default `overview`; `overview`, `clusters`, `projects`, `deploys`, `alerts`, `wallet`), `--read-only`, `--refresh <SECS>` (default `5`) | Interactive terminal UI; without an interactive terminal it fails with `selfhost tui requires an interactive terminal` (exit 2) |
| `selfhost tree` | – | Print every command in the CLI, one per line |
| `selfhost completion` | `<SHELL>`, one of `bash`, `zsh`, `fish` | Generate shell completions |
| `selfhost help` | `[COMMAND]...` | Print help for a command path, e.g. `selfhost help postgres users` |

Bare `selfhost` opens the TUI when stdin and stdout are terminals, `TERM` is not
`dumb`, and `SELFHOSTDEV_NO_TUI` is unset or empty; otherwise it prints help
(`src/cli/tui.rs`).

## Behavior

### Confirmations

Three destructive verbs prompt: `org delete` (type the organization's name back),
`org members remove` and `org invites revoke` (a `[y/N]` question where only
`y`/`yes` passes). `auth login` can also ask, when the profile has no credentials
but the machine holds sign-in credentials from the SelfHost MCP server
(`~/.selfhost/credentials.json`, read only): it asks whether to import them into
the profile (an empty answer means yes) and, on yes, saves the pair and finishes
without a browser hop. The offer is made only on a terminal on stdin or with
`--yes`; without either, `auth login` skips the import and goes straight to the
browser. It falls back to the browser when the import is declined, the file is
missing or unreadable, the pair fails verification, or the verified pair cannot
be saved — the last two warn and continue. `--yes` answers any of these prompts
up front, so nothing is asked.

Without `--yes` and without a terminal on stdin, the command stops before
reading a credential or sending anything (exit 2):

```
usage error: org delete needs confirmation; pass --yes to run it non-interactively
```

A wrong or empty answer / EOF never confirms (`src/cli/mod.rs`):

```
not confirmed: the name did not match; nothing was deleted   # org delete, exit 1
not confirmed; nothing was changed                           # remove / revoke, exit 1
```

### `--dry-run`

Accepted globally but refused everywhere: the gate in the shared dispatch
(`src/cli/mod.rs`, reached from `src/main.rs` even for a bare invocation) runs
before any handler, so with the flag set nothing is sent, written or read and no
store file appears. Exit 2, message on stderr:

```
usage error: dry runs are not supported for organization changes yet; nothing was sent
usage error: dry runs are not supported for api calls yet; nothing was sent
usage error: dry runs are not supported for this command yet; nothing was sent
```

`org` gets `organization changes` (every `org` invocation maps to that subject,
including pure reads such as `org list` or `org roles list --dry-run`, which sends
nothing yet is still refused with that subject), `api` gets `api calls`, and every
other command gets `this command` — `profile`, `auth`, `tui`, `tree`,
`completion`, each stub group, and a bare `selfhost --dry-run`. Nothing prints a request instead of sending it yet.

### Exit codes

| Code | Source |
| --- | --- |
| `0` | Success |
| `1` | API/runtime error; also every `not implemented yet` verb |
| `2` | Usage error: bad flags or arguments, no/unknown organization, unknown profile, confirmation without a TTY, `--dry-run` refusal, `tui` without a terminal, a command group invoked with no subcommand |
| `3` | Not authenticated — run `selfhost auth login` |
| `4` | Billing required (API 402) |
| `75` | Rate limited: one wait for the Retry-After delay (capped at 60 seconds, 1 second when missing) plus exactly one retry, then give up (429) |

`src/error.rs`; README. Errors print to stderr as `<kind>: <message>` —
`usage error: …`, `not authenticated: …`, `billing required: …`,
`rate limited: …`, `not implemented yet: …`.

### Output

Default `table` on a TTY and `json` when stdout is piped. Precedence: `--json`
beats `-o`/`--format`, which beats TTY sniffing (`src/output/mod.rs`). `table`
renders an array of objects as columns, a single object as a `field | value`
sheet, and an empty array as `(no rows)`. Errors go to stderr, so piped stdout
stays parseable.

### Profile and organization resolution

Profile: `--profile` → `SELFHOSTDEV_PROFILE` → the store's default. An unknown
name fails (exit 2):

```
usage error: unknown profile '<name>'; create it first: selfhost profile add <name>
```

That is the wording the profile store raises for `-p`, `org`, `api` and `auth`
(`src/config/store.rs`). The `profile` subcommands check the name themselves
and word it differently (see the `profile` section above).

Organization: the command's own argument → `--org`/`SELFHOSTDEV_ORG` → the
organization saved in the profile (written by `org use`). Only a pid shaped like
`org_` plus hex digits (up to 64 characters) is used as-is; any other `org_…`
string falls through to the slug lookup and can fail as an unknown organization.
Nothing selected (exit 2):

```
usage error: no organization selected; run selfhost org use <slug>
```

A reference that is not one of your organizations (exit 2):

```
usage error: unknown organization '<ref>'; run selfhost org list
```

## Not implemented yet

These groups exist in `--help` and `selfhost tree` but every verb answers
`not implemented yet: <full command path>` on stderr and exits 1:

```sh
selfhost webhook list      # not implemented yet: webhook list
selfhost postgres users list   # not implemented yet: postgres users list
```

| Group | Verbs | `stub_group!` registrations |
| --- | --- | --- |
| `project` | 35 | 9 |
| `deploy` | 27 | 6 |
| `github` | 5 | 3 |
| `domain` | 5 | 1 |
| `postgres` | 54 | 11 |
| `mysql` | 43 | 7 |
| `mongo` | 24 | 3 |
| `redis` | 29 | 4 |
| `clickhouse` | 33 | 5 |
| `opensearch` | 32 | 5 |
| `catalog` | 4 | 1 |
| `billing` | 11 | 2 |
| `cloud` | 6 | 3 |
| `network` | 12 | 4 |
| `ssh-key` | 7 | 4 |
| `alert` | 14 | 4 |
| `scaling` | 8 | 2 |
| `webhook` | 5 | 1 |
| `config` | 5 | 1 |
| **Total** | **359** | **76** |

Verbs = leaf commands under the group in `selfhost tree`; registrations =
`grep -c "stub_group!" src/cli/<group>.rs`. A group invoked with no subcommand
prints its help and exits 2; only the leaf verbs report `not implemented yet`.

The group name is not always the file name: for `ssh-key` the file is
`src/cli/ssh_key.rs`, so substitute the underscored name there.

To do the job of a stub group, call the platform directly with
`selfhost api <endpoint>` (see `references/api.md`).
