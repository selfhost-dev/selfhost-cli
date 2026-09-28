# selfhost-cli

The SelfHost platform CLI: one `selfhost` binary for servers, managed databases,
projects and deployments on [selfhost.dev](https://selfhost.dev).

Standalone repo — it speaks the platform HTTP API and keeps per-profile credentials
under `~/.selfhost/`. Design contract lives in `../selfhost-cli-design.md`
(§3 the `--help` surface, §4 command families, §5 profiles/auth, §6 conventions,
§7 the build plan this repo is being implemented slice by slice).

## Build & run

```sh
cargo build            # -> ./target/debug/selfhost
cargo test             # surface tests for the clap tree
./target/debug/selfhost --help
./target/debug/selfhost tree                     # every command path, one per line
./target/debug/selfhost completion bash          # bash | zsh | fish
```

Requires Rust 1.88+ (MSRV is set by the newest dependencies, `comfy-table` and the
`icu_*` crates pulled in through `reqwest`/`url`).

## Module map

```
src/main.rs        clap parse + process exit-code plumbing
src/cli/           one module per command family, plus the `tree` / `completion` /
                   `help` implementations and the shared argument structs
src/api/           typed HTTP client (envelope, org injection, 429/Retry-After) — Slice 1
src/auth/          browser loopback login (`/mcp-auth`), token cache, MCP import — Slice 1
src/config/        `~/.selfhost/config.json` profile-store types — Slice 1
src/output/        table / json / yaml rendering + exit-code map
src/watch/         poll loops for `--wait` / `--follow` — Slice 6
```

## Status

Scaffold only; see design doc §7 for slices. This commit (Slice 0) registers the whole
documented command surface with real clap subcommands and implements exactly:

- argument parsing, `--help` / `help <cmd>` / `--version`
- `tree` (generic recursion over the clap command tree)
- `completion <bash|zsh|fish>` (clap_complete)
- the output layer: `Format` (`table` / `json` / `yaml`) rendering of `serde_json::Value`
  and the exit-code map (0 ok, 1 error, 2 usage, 3 unauthenticated, 4 billing,
  75 rate-limited)
- the `config` module's serde types for the profile store (types only)

## Deferred

Everything else. Every command that has no implementation yet returns
`not implemented yet: <full command path>` on stderr and exits 1:

```sh
$ selfhost org list
not implemented yet: org list
$ echo $?
1
```

That is the staging mechanism for the remaining slices (1 core client/auth,
2 `postgres` parity, 3 other engines, 4 project plane, 5 account/ops, 6 watch/polish).
