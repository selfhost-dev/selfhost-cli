# selfhost

The command line for SelfHost. One binary to manage databases, projects, and deployments.

This project is young. The full command tree exists and every command has help, but most commands still answer `not implemented yet` until that part is built. Help, shell completions, the tree printer, and the welcome screen already work.

## Install

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

`auth login` hands off to the console and comes back to the CLI when you are done; on a remote machine, pass `--no-browser`, open the printed URL yourself, and paste the redirect URL from the browser's address bar back into the terminal. `auth status` is a checklist and exits 3 when something is missing. `auth token` prints the short-lived access token on stdout only, so keep it out of logs.

Profiles decide where a command points and which credentials it uses. Select one with `--profile` or `SELFHOSTDEV_PROFILE`; add one with `selfhost profile add staging --base-url https://api.staging.example.com`, and make it the default with `selfhost profile use qa`.

In CI or any headless environment, set `FIREBASE_API_KEY` and `FIREBASE_REFRESH_TOKEN` — the same pair the MCP server uses — and no login step is needed.

## Organizations

```sh
selfhost org list                    # every organization you belong to
selfhost org show                    # details for the one you have selected
selfhost org use acme                # choose the one other commands use
selfhost org members list            # the people in it
selfhost org activity list --page 2  # recent activity, newest first
```

`org list` marks the organization other commands use by default and also shows invitations that are still waiting. `org show`, `org members list` and `org activity list` work on that same organization, or on the one you name.

Name an organization by slug (`acme`) or by pid (`org_…`). A pid is used as it is; anything else is looked up among the organizations you belong to, so a slug that does not resolve is a usage error. The organization you name on the command line wins over `--org` and `SELFHOSTDEV_ORG`, which in turn win over the organization saved in the profile; an empty value counts as unset. `org list` and `org use` ignore `--org` — the listing has to work even when the saved organization is gone, and `org use` is how you replace it.

## Commands

| Command | What it is for |
|---|---|
| auth | Sign in, sign out, check the session |
| profile | Saved profiles: endpoints, default org |
| config | Your saved default settings |
| org | Organizations: list, show, select the default, members, activity |
| project | Projects with databases, services, backups |
| deploy | Deploy a repo, watch runs, set env vars and domains |
| github | Connected repos, branches, build settings |
| domain | Custom domains and their DNS status |
| postgres, mysql, mongo, redis, clickhouse, opensearch | Managed databases, one group per engine |
| catalog | Regions, instance types, cost estimates |
| billing | Wallet, top-ups, transactions |
| cloud | Cloud provider credentials |
| network | VPCs, subnets, security groups |
| ssh-key | SSH keys for the org and projects |
| alert | Alert rules and fired alerts |
| scaling | Scaling policies and capacity plans |
| webhook | Webhook endpoints for the org |
| tui | The interactive terminal UI |
| tree | Every command, one per line |
| completion | Shell completions for bash, zsh, fish |

Every command takes the same global flags: `--profile`, `--base-url`, `--org`, `-o/--format`, `--json`, `--no-color`, `--timeout`, `--poll-interval`, `-y/--yes`, `--dry-run`, `-q/--quiet`, `-v/--verbose`, `--debug`.

## License

Apache 2.0. See LICENSE for the full text.
