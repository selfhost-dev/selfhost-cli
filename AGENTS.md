# AGENTS.md

Working rules for coding agents in this repo. Humans may override any rule with an explicit instruction.

## Worktrees

- Work in a git worktree, never on `main`: `git worktree add -b <branch> <path>` before you change anything. Put the worktree at the parent directory level, never inside the repo, and run `pwd && git branch` before your first edit to confirm which tree you are in.

## Commits

- Format: Conventional Commits — `type(scope): description`, e.g. `feat(tui): open welcome screen on bare invocation`.
- The description communicates the **behavior a user gets**, not the diff. The diff already says what changed; the message says what is now true for the user.
  - Good: `fix(cli): engine help lists only commands the platform supports`
  - Bad: `fix(cli): prune reload-users, update tests, sync doc` (restates the diff)
- One logical change per commit. Do not force-push or rewrite commits that are not yours.
- Banned in commit messages: the word `verbatim`, and process/internal labels (`round 1`, `verifier round`, `slice N`, `WIP`). Describe the change, not the workflow that produced it.

## Releases

A `v*` tag is the only thing that publishes. No one uploads an artifact by hand, and the tag must point at a commit on `main`.

1. Land the change through a pull request into `main`.
2. Open a separate pull request that bumps `Cargo.toml` and `Cargo.lock` and nothing else, titled `chore(release): selfhost reports version X.Y.Z`.
3. Create a signed annotated tag `vX.Y.Z` on that merge commit and push it. Tag creation is admin-only by ruleset, and a tag that disagrees with the `Cargo.toml` version fails the run.
4. Verify the run: every job green, thirteen assets on the GitHub release, `https://cli.selfhost.dev/latest.json` reporting the new version, and all six platform digests matching the release artifacts.

CI builds six native targets, publishes the GitHub release, uploads the distribution, and regenerates the manifest. A version containing a dash publishes binaries but deliberately leaves the manifest pointing at the stable release.

## Public surfaces and identifiers

- This repository is public. Never write an AWS account ID, a role ARN, a credential, or a token into a file, a commit message, a pull request, a workflow run log, or a release note.
- Never put an AWS organization ID, an AWS account ID, or any other organization identifier into a pull request naked — not in the title, the body, a comment, a pasted log, a diff, or a screenshot. Show it masked (`****1234`) or name the organization instead. Issues, discussions, and release notes are the same public surface.
- A history rewrite does not take an identifier off GitHub. Pull request diffs, their `refs/pull/<n>/head` refs, workflow run logs, and release pages keep serving it after the branch is clean. Treat everything pushed as permanent.

## Test cases

- Test names describe the **behavior under test**, e.g. `pool_commands_match_platform_capabilities`, `bare_invocation_opens_tui_on_a_tty`.
- Banned in test names: the word `verbatim`. (Byte-for-byte help comparisons are fine as assertions — name them for the behavior, e.g. `help_block_matches_the_running_binary`.)

## Before pushing changes that touch Rust files

Run all three and paste the results. Never push on an unrun gate.

1. `cargo test` — every target green, zero failures. Quote the per-target pass counts.
2. `cargo fmt --check` — clean, or you know exactly which flagged files are pre-existing. The repo carries
   formatting drift on `main` (currently `src/auth/loopback.rs`, `src/auth/mod.rs`, `src/cli/auth.rs`,
   `src/cli/profile.rs`, `src/config/store.rs`, `src/output/mod.rs`, `tests/auth_flow.rs`), so a bare
   failure proves nothing. Read the `Diff in …` lines, and if any flagged file is one you edited, run
   `cargo fmt` and commit the result. Re-check that list after `git stash push --include-untracked -- src tests`
   and `git stash pop` if you need to prove the drift predates your work.
3. `cargo clippy --all-targets` — no new warnings in the files you touched.

Two traps worth knowing:

- `rustfmt --check <single-file>` recurses through the crate and reports sibling files it merely walks
  past. It makes clean files look dirty. Use `cargo fmt --check` and read the `Diff in` headers.
- A green test run is not a licence to push. If the working tree also holds changes that are not yours,
  say so and commit them separately — one logical change per commit still applies at push time.

## Help text

- Client-facing voice: plain customer language, no backticked command names, no internal jargon (no `poll cadence`, `HTTP request`, `429`, process names). The test suite enforces this; keep it green.
