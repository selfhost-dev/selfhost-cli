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

## CloudFront access logs

The distribution writes classic access logs to `selfhost-cli-cf-logs-<account>`
under the `cloudfront/` prefix. That bucket is **not** part of the
CloudFormation stack: classic logging needs an ACL grant to the S3 log
delivery group, which in turn needs two public-access-block switches relaxed
on that one bucket. Left outside the stack, a stack update can never tighten
them back and silently stop log delivery.

Recreating it by hand, in this order:

```sh
bucket=selfhost-cli-cf-logs-<account>
aws s3api create-bucket --bucket "$bucket" --region us-east-1
aws s3api put-public-access-block --bucket "$bucket" \
  --public-access-block-configuration \
  BlockPublicAcls=false,IgnorePublicAcls=false,BlockPublicPolicy=true,RestrictPublicBuckets=true
aws s3api put-bucket-ownership-controls --bucket "$bucket" \
  --ownership-controls 'Rules=[{ObjectOwnership=BucketOwnerPreferred}]'
aws s3api put-bucket-acl --bucket "$bucket" \
  --grant-write URI='http://acs.amazonaws.com/groups/s3/LogDelivery' \
  --grant-read-acp URI='http://acs.amazonaws.com/groups/s3/LogDelivery'
```

Then allow `s3:PutObject` from `cloudfront.amazonaws.com` on `cloudfront/*`,
scoped by `AWS:SourceArn` to this account's distributions. Both switches must
stay `false` or delivery stops; the bucket holds logs only and is never
publicly readable.

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
