# distribution/

Files that install `selfhost` on user machines. Everything here is served
from `https://cli.selfhost.dev` (S3 + CloudFront, wired by hand — there is no
CloudFormation stack; the release workflow uploads on a tag push).

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

## How cli.selfhost.dev is wired

- **Bucket** `selfhost-cli-distribution-prod` (ap-south-1), all public access
  blocked. Only the CDN reads it, through the origin access control
  `selfhost-cli-s3-oac` and a bucket policy scoped to the distribution's ARN.
  A missing object therefore answers `403`, not `404`.
- **Distribution** `E37YM4KZPXQ3TV`, alias `cli.selfhost.dev`, ACM certificate
  for that name issued in us-east-1, viewer protocol `redirect-to-https`, and
  the managed **CachingOptimized** policy — which honours the origin's
  `Cache-Control`, so the TTLs above survive. CloudFront access logging is off.
- **DNS** — a `cli` CNAME in Cloudflare, **DNS only** (grey cloud), pointing at
  the distribution domain. Proxying it would put Cloudflare in front of
  CloudFront and break the origin pin below.
- The certificate is validated by a leftover ACM validation CNAME in the same
  zone; re-requesting a certificate for this name normally reuses that record.

## How a release reaches the bucket

Pushing a `v*` tag runs `.github/workflows/release.yml`: it builds the six
binaries, publishes the GitHub Release, and then the `upload-distribution` job
does the rest.

- It assumes the role `selfhost-cli-distribution-publisher` through GitHub's
  OIDC provider. The role ARN lives in the repository **secret**
  `AWS_ROLE_ARN`, never in the workflow file — an ARN carries the AWS account
  ID and this repository is public. The role trusts only tag pushes in this
  repository, and it may only put objects in the bucket (plus invalidate the
  four mutable pointers).
- It uploads the binaries under `v<version>/` only after proving the path does
  not exist, so a write-once path is never overwritten.
- It regenerates `latest.json` from the artifacts it just uploaded, so the
  manifest cannot drift from the binaries, and flips the installers with it —
  then invalidates `/latest.json`, `/install.sh`, `/install.ps1` and
  `/install.cmd`, so the flip is immediate instead of up to five minutes late.
- A pre-release version (`0.2.0-rc1` and the like) publishes its binaries but
  leaves the manifest pointing at the stable release.

## Bootstrap and emergencies

If the pipeline is unavailable, the same layout can be published by hand.

1. Installers and manifest, with the 5-minute TTL:

   ```sh
   aws s3 cp distribution/install.sh  s3://selfhost-cli-distribution-prod/install.sh  --content-type text/x-shellscript --cache-control "public, max-age=300"
   aws s3 cp distribution/install.ps1 s3://selfhost-cli-distribution-prod/install.ps1 --content-type text/plain        --cache-control "public, max-age=300"
   aws s3 cp distribution/install.cmd s3://selfhost-cli-distribution-prod/install.cmd --content-type text/plain        --cache-control "public, max-age=300"
   aws s3 cp distribution/latest.json s3://selfhost-cli-distribution-prod/latest.json --content-type application/json   --cache-control "public, max-age=300"
   ```

2. Binaries under a new `v<version>/` prefix, with the one-year immutable TTL:

   ```sh
   for f in selfhost-linux-x86_64 selfhost-linux-aarch64 selfhost-macos-x86_64 \
            selfhost-macos-aarch64 selfhost-windows-x86_64.zip selfhost-windows-aarch64.zip; do
     aws s3 cp "$f" "s3://selfhost-cli-distribution-prod/v0.1.0/$f" \
       --content-type application/octet-stream \
       --cache-control "public, max-age=31536000, immutable"
   done
   ```

3. Point `latest.json` at the new version — `version`, every `assets` URL and
   the matching `sha256` — then re-upload it (step 1).

4. Verify what a user gets:

   ```sh
   curl -fsSL https://cli.selfhost.dev/latest.json | jq -r .version
   curl -fsSL https://cli.selfhost.dev/v0.1.0/selfhost-macos-aarch64 | shasum -a 256
   ```

   The digest must equal the manifest's `sha256.macos-aarch64`.

The GitHub Release stays the durable record of every asset; this bucket is the
install path users actually hit.

## Manifest shape

```json
{
  "version": "0.1.0",
  "assets": { "linux-x86_64": "https://cli.selfhost.dev/v0.1.0/…", … },
  "sha256": { "linux-x86_64": "…64 hex chars…", … }
}
```

Keys are `<os>-<arch>` with `linux|macos|windows` × `x86_64|aarch64` — six
platforms. The installers detect the same triple, refuse a manifest that points
anywhere other than `https://cli.selfhost.dev/…`, and fail loudly on a platform
the manifest does not cover or whose checksum is not 64 hex characters.
