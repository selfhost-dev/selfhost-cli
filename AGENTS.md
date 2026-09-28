# AGENTS.md

Working rules for coding agents in this repo. Humans may override any rule with an explicit instruction.

## Commits

- Format: Conventional Commits — `type(scope): description`, e.g. `feat(tui): open welcome screen on bare invocation`.
- The description communicates the **behavior a user gets**, not the diff. The diff already says what changed; the message says what is now true for the user.
  - Good: `fix(cli): engine help lists only commands the platform supports`
  - Bad: `fix(cli): prune reload-users, update tests, sync doc` (restates the diff)
- One logical change per commit. Do not force-push or rewrite commits that are not yours.
- Banned in commit messages: the word `verbatim`, and process/internal labels (`round 1`, `verifier round`, `slice N`, `WIP`). Describe the change, not the workflow that produced it.

## Test cases

- Test names describe the **behavior under test**, e.g. `pool_commands_match_platform_capabilities`, `bare_invocation_opens_tui_on_a_tty`.
- Banned in test names: the word `verbatim`. (Byte-for-byte help comparisons are fine as assertions — name them for the behavior, e.g. `help_block_matches_the_running_binary`.)

## Help text

- Client-facing voice: plain customer language, no backticked command names, no internal jargon (no `poll cadence`, `HTTP request`, `429`, process names). The test suite enforces this; keep it green.
