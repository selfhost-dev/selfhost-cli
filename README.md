# selfhost

The command line for SelfHost. One binary to manage databases, projects, and deployments.

This project is young. The whole command tree is registered and every command has help, but the ones marked ⏳ in the [command list](#commands) answer `not implemented yet` until that part is built. Sign-in, profiles, organizations, raw API calls, help, completions, the tree printer, the welcome screen and `selfhost update` work today.

## Install

### macOS and Linux

```sh
curl -fsSL https://cli.selfhost.dev/install.sh | sh
```

The installer works out your platform and downloads the matching build from `cli.selfhost.dev`. It checks the SHA-256 against the release manifest before it writes the binary to `~/.local/bin`. If that directory is not on your PATH, the installer prints what to add. You need `curl`, `awk`, and one of `sha256sum`, `shasum` or `openssl`.

To install somewhere else, name the directory on `sh`, the command at the end of the pipe. Put it in front of `curl` and it never reaches the installer:

```sh
curl -fsSL https://cli.selfhost.dev/install.sh | SELFHOST_INSTALL_DIR=/usr/local/bin sh
```

### Windows

In PowerShell:

```powershell
irm https://cli.selfhost.dev/install.ps1 | iex
```

It unpacks `selfhost.exe` into `%LOCALAPPDATA%\Programs\selfhost` and adds that directory to your user PATH. In Command Prompt, run `install.cmd`, which starts the same script through PowerShell.

### Upgrading and pinning a version

```sh
selfhost update             # install the newest release over the one you have
selfhost update --check     # just say whether there is one
```

`update` downloads the build for your platform, checks it against the SHA-256 the release list publishes, and replaces the running binary. It never asks for administrator rights. If it cannot write where the binary sits, it prints the installer command that can and stops. Set `SELFHOST_MANIFEST_URL` to read a different release list. The installer honours the same name.

The installer stays the way to install into a directory the CLI cannot write, and to move the binary somewhere else in the first place. It has no version pin, so it never fetches a pre-release, and `update` follows the same rule and leaves a pre-release build alone. Each pre-release tag publishes its own binaries on the [releases page](https://github.com/selfhost-dev/selfhost-cli/releases). Download the file for your platform and put it on your PATH.

### Checking what you installed

```sh
selfhost --version
shasum -a 256 "$(command -v selfhost)"   # must match the digest in https://cli.selfhost.dev/latest.json
```

### Uninstalling

Delete the binary and, on Windows, drop the install directory from your user PATH.

### Building from source

You need Rust 1.88 or newer.

```sh
cargo build
./target/debug/selfhost --help
```

## Try it

```sh
selfhost --help             # everything it can do
selfhost postgres --help    # one database engine
selfhost tree               # every command, one per line
selfhost completion bash    # also zsh and fish
selfhost tui                # welcome screen, needs a terminal
```

Running bare `selfhost` on a terminal opens the welcome screen too. Press q to quit. Set `SELFHOSTDEV_NO_TUI=1` to get plain help output instead.

## Point it somewhere

The built-in profiles are `prod`, `qa`, and `local`. Pick one, or pass a URL per command.

```sh
selfhost --profile qa postgres list
selfhost --base-url http://localhost:3000 postgres list
```

Flags have env var equivalents: `SELFHOSTDEV_PROFILE`, `SELFHOSTDEV_BASE_URL`, `SELFHOSTDEV_ORG`.

Output is a table on a terminal and JSON when piped. Force it with `-o table|json|yaml` or `--json`. Commands that need a sign-in stop with exit 3 until you run `selfhost auth login`.

## Sign in

```sh
selfhost auth login              # opens your browser to finish sign-in
selfhost auth login --no-browser # print the URL instead, for a remote machine
selfhost auth status             # what the current profile can do
selfhost auth logout             # drop the saved credentials
selfhost auth token              # print the access token for scripts
```

`auth login` hands off to the console and comes back to the CLI when you are done. On a remote machine, pass `--no-browser`, open the printed URL yourself, and paste the redirect URL from the browser's address bar back into the terminal. `auth status` is a checklist and exits 3 when something is missing. `auth token` prints the short-lived access token on stdout only, so keep it out of logs.

Profiles decide where a command points and which credentials it uses. Select one with `--profile` or `SELFHOSTDEV_PROFILE`, add one with `selfhost profile add staging --base-url https://api.staging.example.com`, and make it the default with `selfhost profile use qa`.

In CI or any headless environment, set `FIREBASE_API_KEY` and `FIREBASE_REFRESH_TOKEN` and no login step is needed. They are the same pair the MCP server uses.

## Organizations

```sh
selfhost org list                    # every organization you belong to
selfhost org show                    # details for the one you have selected
selfhost org use acme                # choose the one other commands use
selfhost org create "Acme Labs" --description "Staging and demos"
selfhost org update --name "Acme Labs EU"
selfhost org delete                  # asks for the name first
selfhost org members list            # the people in it
selfhost org members add dana@example.com --role manager
selfhost org members update-role dana@example.com --role admin
selfhost org members remove dana@example.com
selfhost org roles list              # what each role can do
selfhost org invites list            # invitations and where they stand
selfhost org invites create dana@example.com
selfhost org invites revoke dana@example.com
selfhost org activity list --page 2  # recent activity, newest first
```

`org list` marks the organization other commands use by default and also shows invitations that are still waiting. `org show`, `org members list`, `org invites list` and `org activity list` work on that same organization, or on the one you name. `org create` takes the organization only from the command line, so the selected organization means nothing to it. The caller owns whatever they create.

`org create` names the new organization and the platform derives the slug from the name. The slug is never something you pass, and `org show` is where you read it. `org update` changes the name or the description, and needs at least one of them.

Invitations arrive by email. There is no separate add-member call. `org members add` and `org invites create` do the same thing, and the person becomes a member when they accept the invitation. `org invites list` shows every invitation and its status, not only the ones still waiting. Managing members and invitations needs owner or admin rights on the organization, and the API refuses anything a role cannot do with a message the CLI prints as-is.

`org roles list` is printed by the CLI itself. The platform ships five system roles, owner, admin, billing, manager and member, and has no endpoint that returns them. A role added on the server needs a newer CLI to appear here.

Deleting an organization, removing a member and cancelling an invitation are destructive, so they ask first. `org delete` wants the organization's name typed back, and the other two want a yes. `--yes` answers the prompt for scripts. Without it, a run with no terminal stops and tells you to pass `--yes`, rather than waiting on a prompt that can never arrive.

Every row of `org list` and the details `org show` prints also say which profile they came from, so a machine with several saved profiles is easy to tell apart.

Name an organization by slug (`acme`) or by pid (`org_…`). A pid is used as it is. Anything else is looked up among the organizations you belong to, so a slug that does not resolve is a usage error. The organization you name on the command line wins over `--org` and `SELFHOSTDEV_ORG`, which in turn win over the organization saved in the profile. An empty value counts as unset. `org list` and `org use` ignore `--org`, because the listing has to work even when the saved organization is gone, and `org use` is how you replace it.

## Commands

⏳ means the command is registered but answers `not implemented yet`. Everything else in the list works today.

| Command | What it is for |
|---|---|
| auth | Sign in, sign out, check the session |
| profile | Saved profiles: endpoints, default org |
| org | Organizations: create, list, show, update, delete, members, roles, invitations, activity |
| api | Call any platform endpoint directly |
| ⏳ config | Your saved default settings |
| ⏳ project | Projects with databases, services, backups |
| ⏳ deploy | Deploy a repo, watch runs, set env vars and domains |
| ⏳ github | Connected repos, branches, build settings |
| ⏳ domain | Custom domains and their DNS status |
| ⏳ postgres, mysql, redis, clickhouse, opensearch | Managed databases, one group per engine |
| ⏳ catalog | Regions, instance types, cost estimates |
| ⏳ billing | Wallet, top-ups, transactions |
| ⏳ cloud | Cloud provider credentials |
| ⏳ network | VPCs, subnets, security groups |
| ⏳ ssh-key | SSH keys for the org and projects |
| ⏳ alert | Alert rules and fired alerts |
| ⏳ scaling | Scaling policies and capacity plans |
| ⏳ webhook | Webhook endpoints for the org |
| tui | The interactive terminal UI |
| tree | Every command, one per line |
| help | Help for any command |
| completion | Shell completions for bash, zsh, fish |
| update | Move this CLI to the newest published build |

Every command takes the same global flags: `--profile`, `--base-url`, `--org`, `-o/--format`, `--json`, `--no-color`, `--timeout`, `--poll-interval`, `-y/--yes`, `--dry-run`, `-q/--quiet`, `-v/--verbose`, `--debug`.

No command previews a request yet. A command run with `--dry-run` stops before it does anything and says nothing was sent, so the flag can never be mistaken for a safety net.

## Raw API access

```sh
selfhost api /organizations/{org}/members --org acme
selfhost api -X GET /organizations -f per_page=50
selfhost api -X PATCH /organizations/{org} -F description="Staging and demos"
echo '{"description":"x"}' | selfhost api -X PATCH /organizations/{org} --input -
selfhost api /organizations/{org} -i --format json
```

`api` calls any platform endpoint directly, with the profile's sign-in and organization handling, so it works under any profile. It defaults to GET and switches to POST when parameters or a body are present, unless a method is passed with `-X`. `-f` adds plain text parameters and `-F` adds typed ones, where a value starting with `@` reads a file or stdin. On a GET they become query parameters, on a write they become the JSON body, and when `--input` sends a raw body from a file or stdin they move to the query instead. `{org}` in the path is filled in with the resolved organization. `-i` includes the status line and headers in the output. `--silent` prints nothing and leaves the exit code to say how it went. `-H` adds extra headers, except the ones the CLI manages itself, which are authorization, accept, content type and content length. Those fail the command instead of being overridden or silently dropped. Output and exit codes behave like every other command.

## License

Apache 2.0. See LICENSE for the full text.
