# distribution/

Files that install `selfhost` on user machines. Everything here is served
from `https://cli.selfhost.dev` (S3 + CloudFront; infrastructure in
`infra/aws/distribution.yaml`).

| File | Purpose |
|---|---|
| `install.sh` | POSIX sh installer for Linux and macOS (`curl -fsSL https://cli.selfhost.dev/install.sh \| sh`) |
| `install.ps1` | PowerShell installer for Windows (`irm https://cli.selfhost.dev/install.ps1 \| iex`) |
| `install.cmd` | cmd.exe/double-click shim that runs `install.ps1` |
| `latest.json` | Release manifest: current version, per-platform download URLs, SHA-256 digests |

## Cache model (the part that must stay true)

- `latest.json`, `install.sh`, `install.ps1` are **mutable pointers** with a
  5-minute `Cache-Control` TTL. They change on every release.
- `/v<version>/…` binaries are **write-once, immutable**, cached for a year.
  A release never overwrites an existing versioned path; it flips the
  manifest pointer instead.

Breaking either rule makes the 5-minute TTL unsafe. Don't.

## Manifest shape

```json
{
  "version": "0.1.0",
  "assets": { "linux-x86_64": "https://cli.selfhost.dev/v0.1.0/…", … },
  "sha256": { "linux-x86_64": "…64 hex chars…", … }
}
```

Keys are `<os>-<arch>` with `linux|macos|windows` × `x86_64|aarch64` — six
platforms. The installers detect the same triple and fail loudly on a
platform the manifest does not cover.

The upload job in `.github/workflows/release.yml` regenerates this file on
every tagged release and pushes it to S3; hand edits here are for bootstrap
and emergencies only, and are overwritten by the next release.
