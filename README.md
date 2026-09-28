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

Running bare `selfhost` on a terminal opens the welcome screen too. Press q to quit. Set `SELFHOST_NO_TUI=1` to get plain help output instead.

## Point it somewhere

The built-in profiles are `prod`, `qa`, and `local`. Pick one, or pass a URL per command.

```sh
selfhost --profile qa postgres list
selfhost --base-url http://localhost:3000 postgres list
```

Flags have env var equivalents: `SELFHOST_PROFILE`, `SELFHOST_BASE_URL`, `SELFHOST_ORG`.

Output is a table on a terminal and JSON when piped. Force it with `-o table|json|yaml` or `--json`. Sign in is not wired up yet, so authenticated commands still stop at `not implemented yet` and exit 1.

## Commands

| Command | What it is for |
|---|---|
| auth | Sign in, sign out, check the session |
| profile | Saved profiles: endpoints, default org |
| config | Your saved default settings |
| org | Organizations, members, invitations |
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
