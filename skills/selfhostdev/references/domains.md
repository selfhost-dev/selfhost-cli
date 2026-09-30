# Custom domains

Point your own hostname (`app.example.com`) at either a deployment or a template
service, and prove you own it. The typed commands (`selfhost domain ...`,
`selfhost deploy domain ...`, `selfhost project service custom-domain ...`) all
answer `not implemented yet: <group> <verb>`, so provision through
`selfhost api` with the paths below. There are two halves: the **org-level
library** (`CustomDomainVerification`, `/api/v1/platform/custom_domains`, keyed
by the domain string, shared by every deployment in the org) and the
**attachment routes** that bind a domain to one deployment or one service. The
org library and the deployment routes are scoped by an `organization_id`
parameter; the service route derives its org from `project_id` (see
[api.md](api.md) for the injection rule).

## Before you start

- Signed in: `selfhost auth status`; an organization resolved
  (`selfhost org use <slug>`, `--org`, or `SELFHOSTDEV_ORG`) — the org library
  and the deployment routes read `organization_id` and 404 without it; the
  service route picks up the org from its `project_id`.
- `selfhost api` rules you rely on here ([api.md](api.md)): a bare call is
  `GET`; any `-f`/`-F`/`--input` makes it a `POST` unless `-X` says otherwise.
  A POST **with no body needs `-X POST`** — this matters for `verify`.
  `-f` sends raw strings, `-F` types values.
- The org is injected as `organization_id`: appended to the query on GET, into
  the JSON body on writes (both are read by the controllers). Paths below carry
  the literal `/api/v1/platform/...` prefix; no `{org}` placeholder is used.
- A deployment must be provisioned (`coolify_app_uuid` present) before a domain
  can be attached (`app/controllers/api/v1/platform/github_repo_deployments_controller.rb:1065`).

## Endpoint map

### Org library — `selfhost domain ...`

| Future CLI command | Method and path | What it does | Key parameters | Source |
|---|---|---|---|---|
| `selfhost domain list` | `GET /api/v1/platform/custom_domains` | Every verification record for the org, newest first | — | `config/routes.rb:103`, `app/controllers/api/v1/platform/custom_domains_controller.rb:18` |
| `selfhost domain add` | `POST /api/v1/platform/custom_domains` | Submits a domain; returns the DNS records to create. Re-submitting resets the token for a not-yet-verified domain; a verified domain keeps its token and status until deleted | `domain` | `config/routes.rb:103`, `app/controllers/api/v1/platform/custom_domains_controller.rb:30` |
| — (read-back; no verb) | `GET /api/v1/platform/custom_domains/:domain` | DNS records + status for one domain (no token) | — | `config/routes.rb:106`, `app/controllers/api/v1/platform/custom_domains_controller.rb:56` |
| `selfhost domain verify` | `POST /api/v1/platform/custom_domains/:domain/verify` | Live TXT + CNAME lookup; moves the status | — | `config/routes.rb:108`, `app/controllers/api/v1/platform/custom_domains_controller.rb:72` |
| `selfhost domain remove` | `DELETE /api/v1/platform/custom_domains/:domain` | Destroys the verification record | — | `config/routes.rb:106`, `app/controllers/api/v1/platform/custom_domains_controller.rb:165` |
| `selfhost domain sync` | — | No endpoint; verification itself is the sync (see Day-2) | — | — |

Response fields — `create`: `custom_domain`, `status`, `dns_records`
(`[{type,name,value}]`), `verification_token`. `list`:
`custom_domains[].{domain,status,verified_at,created_at}`. `show`:
`custom_domain`, `status`, `verified_at`, `dns_records`. A fresh `verify`
that reaches a decision returns the record plus
`checks.{txt,cname}.{verified,expected,found}` (the `txt` check also carries
`hostname`); re-verifying an already-verified domain returns just the record
with no `checks` key, and a failed `verify` (422) returns `custom_domain`,
`status`, `records_found`, and `checks`. Sources:
`app/controllers/api/v1/platform/custom_domains_controller.rb:21-25` (list),
`:41-49` (create), `:58-65` (show), `:108-158` (verify), `:167-170` (destroy),
`:243-250` (`verification_json`).

### Deployment attachment — `selfhost deploy domain ...`

Paths exist for both `/deployments/:id` and the alias
`/github_repo_deployments/:id` (same controller, same params).

| Future CLI command | Method and path | What it does | Key parameters | Source |
|---|---|---|---|---|
| `selfhost deploy domain list` | `GET /api/v1/platform/deployments/:id` | Read-only; returns the deployment plus `domains[].{domain,is_primary,status,url,dns_records}` | — | `config/routes.rb:120,146`, `app/controllers/api/v1/platform/github_repo_deployments_controller.rb:1038,1146` |
| `selfhost deploy domain add` | `POST /api/v1/platform/deployments/:id/custom_domain` | Appends one domain (primary), creates its org verification if new | `domain` | `config/routes.rb:153`, `app/controllers/api/v1/platform/github_repo_deployments_controller.rb:531` |
| (same action) | `POST /api/v1/platform/deployments/:id/domains` | Same action; body field is still `domain` | `domain` | `config/routes.rb:154` |
| `selfhost deploy domain remove` | `DELETE /api/v1/platform/deployments/:id/domains/:domain` | Removes one domain | — | `config/routes.rb:156`, `app/controllers/api/v1/platform/github_repo_deployments_controller.rb:552` |
| (same action) | `DELETE /api/v1/platform/deployments/:id/custom_domain` | Removes every domain (`domain` in the body removes one) | `domain` (optional) | `config/routes.rb:158` |
| `selfhost deploy domain` (set) | `PATCH /api/v1/platform/deployments/:id/domains` | **Replaces** the whole domain set | `domains` | `config/routes.rb:155`, `app/controllers/api/v1/platform/github_repo_deployments_controller.rb:280` |
| `selfhost deploy domain verify` | `POST /api/v1/platform/custom_domains/:domain/verify` | Same org-level verify | — | `config/routes.rb:108` |
| `selfhost deploy domain sync` | `PATCH /api/v1/platform/deployments/:id/domains` | Re-push verified domains to Coolify (re-runs the sync) | `domains` | `app/controllers/api/v1/platform/github_repo_deployments_controller.rb:280,1064` |

Add/remove return `custom_domain`, `deploy_url`, and
`domains[].{domain,is_primary,status,url,dns_records}`
(`app/controllers/api/v1/platform/github_repo_deployments_controller.rb:1146`). `status` is `pending` or
`verified` (`app/models/deployment_domain.rb:5`). `dns_records` is null when
the org-level verification row for that domain is gone (deleting the org
record does not cascade to deployment domains), so treat a null as
not-verifiable until the domain is submitted again.

### Service attachment — `selfhost project service custom-domain ...`

| Future CLI command | Method and path | What it does | Key parameters | Source |
|---|---|---|---|---|
| `selfhost project service custom-domain add` | `POST /api/v1/platform/projects/:project_id/services/:pid/custom_domain` | Requires the domain to already resolve to the project IP; pins it on the web component and redeploys | `domain` | `config/routes.rb:73`, `app/controllers/api/v1/platform/services_controller.rb:337` |
| `selfhost project service custom-domain remove` | `DELETE /api/v1/platform/projects/:project_id/services/:pid/custom_domain` | Re-pins the platform domain | — | `config/routes.rb:74`, `app/controllers/api/v1/platform/services_controller.rb:375` |

Both return `data.service` (the service payload); the domain state to poll is
`custom_domain`, `custom_domain_status` (`pending`, `verifying`, `active`,
`failed`), `custom_domain_error`, plus `external_url`/`platform_url`
(`app/models/coolify_service.rb:195-262`). Service routes ignore `organization_id` — the
project (`project_id`, a `CoolifyProject` pid) supplies the org
(`app/controllers/api/v1/platform/services_controller.rb:479`).

## Add and verify one end to end

Org-level flow, read-only after the two writes:

```sh
# 1. Submit the domain — mutating. Response: dns_records + verification_token.
selfhost api /api/v1/platform/custom_domains -f domain=app.example.com -o json

# 2. Read the records again at any time (show omits the token).
selfhost api /api/v1/platform/custom_domains/app.example.com -o json
```

`dns_records` is always two rows (`app/models/custom_domain_verification.rb:68-72`):

| type | name | value |
|---|---|---|
| `CNAME` | `@` | the CNAME target for the org |
| `TXT` | `_selfhost-verify` | `verification_token` (`selfhost-verify=<hex>`) |

Create both at the DNS provider. The TXT host to publish is
`_selfhost-verify.app.example.com` (`app/models/custom_domain_verification.rb:76-78`).
The CNAME target is the org's first deployment's `deploy_url` host, or the
literal `your-app.selfhost.dev` when the org has none
(`app/controllers/api/v1/platform/custom_domains_controller.rb:209-220`).

```sh
# 3. Check both records live — mutating state. NO body, so -X POST is required.
selfhost api -X POST /api/v1/platform/custom_domains/app.example.com/verify -o json
```

`verify` resolves TXT and CNAME with `Resolv` and flips the status
(`app/controllers/api/v1/platform/custom_domains_controller.rb:88-160`):

| TXT | CNAME | HTTP | `data.status` | Meaning |
|---|---|---|---|---|
| ok | ok | 200 | `verified` | Done; `sync_deployment_domains!` runs |
| ok | — | 200 | `ownership_verified` | Keep the CNAME |
| — | ok | 200 | `routing_configured` | Keep the TXT |
| — | — | 422 | `pending_verification` | Nothing published (`records_found: false`) — add the records |
| present | present | 422 | `pending_verification` | Records published but wrong (`records_found: true`) — compare `checks` |

Partial states return **exit 0**; treat only `verified` as complete.

```sh
# 4. Confirm and attach — the domain only serves once a deployment carries it
#    and the org record is verified. Add it to the deployment first if needed:
selfhost api /api/v1/platform/deployments/<deployment_pid>/custom_domain \
  -f domain=app.example.com -o json

# 5. Poll the org list for status; the deployment show for custom_domain/deploy_url.
selfhost api /api/v1/platform/custom_domains -o json
selfhost api /api/v1/platform/deployments/<deployment_pid> -o json

# 6. Teardown — destructive.
selfhost api -X DELETE /api/v1/platform/custom_domains/app.example.com
```

On `verified`, `mark_verified!` marks every matching `DeploymentDomain` in the
org as `verified` and re-syncs each deployment to Coolify with
`instant_deploy` (`app/models/custom_domain_verification.rb:44-105`); that is what makes
Traefik route the host and Let's Encrypt issue the certificate. The org-level
verify itself does DNS checks only — it issues nothing.

**Service flow differs:** create the A record first, then add.

```sh
# Mutating. A 422 here is the DNS gate: it carries the A-record instruction.
selfhost api /api/v1/platform/projects/<project_pid>/services/<service_pid>/custom_domain \
  -f domain=app.example.com -o json

# Poll the service for custom_domain_status — the add enqueued the verifier.
selfhost api /api/v1/platform/projects/<project_pid>/services/<service_pid> -o json
```

The service route refuses to apply until `app.example.com` resolves to the
project's public IP (A record, or CNAME chain); otherwise it returns 422 with
`dns_instructions` (`app/services/service_provisioning/custom_domain_applier.rb:44-58`, `app/controllers/api/v1/platform/services_controller.rb:357`).
On success the add is a mutation that flips `custom_domain_status` to
`verifying` and enqueues `CoolifyServiceDomainVerificationJob`
(`app/services/service_provisioning/custom_domain_applier.rb:62-95`). That job re-polls `https://<domain>/` with
**certificate verification on** every 15 s for up to 15 minutes, then writes
`active` or `failed` + `custom_domain_error` — never touching `service.status`
(`app/jobs/coolify/service_domain_verification_job.rb:15-47,82`). A Traefik self-signed
interim cert reads as not-yet-issued, which is the intended signal.

## Day-2 operations

| Operation | Call | Notes |
|---|---|---|
| Re-read the token | `POST` create again | `submit!` regenerates the token for a non-verified record (`app/models/custom_domain_verification.rb:28`) |
| Reset a verified domain | `DELETE` then `POST` | `submit!` no-ops once `verified`; the old TXT becomes invalid |
| Attach a verified domain as primary | `POST .../custom_domain` | Appends; marks it primary, demotes the previous primary (`app/controllers/api/v1/platform/github_repo_deployments_controller.rb:1064-1120`) |
| Replace the domain set | `PATCH .../domains` | `append: false` destroys all first; send a raw body for several: `printf '{"domains":["a.example.com","b.example.com"]}' \| selfhost api -X PATCH .../domains --input -` |
| Partial verification can move back | `POST` create again, or `POST .../:domain/verify` | Re-submitting a partially-verified domain resets it to pending and clears the verified time, and each `verify` re-marks the status from the records that resolve now — so a domain can flip between ownership-verified and routing-configured as records change. Only a fully verified domain stays put |
| Certificate state | poll the owner | Deployments: `deploy_url` / `domains[].url`; services: `tls_status` + `custom_domain_status` (`app/models/coolify_service.rb:204-262`) |
| Set the domain at create | `POST /api/v1/platform/projects/:project_id/services/:template_type` with `custom_domain` | Stored `pending` (no apply yet) and pinning runs when the service turns ready; see [services.md](services.md) (`app/jobs/provision_service_job.rb:155-172`) |

## Gotchas

- **Route constraint on `:domain`.** Org show/verify/destroy sit under
  `constraints(domain: /[a-z0-9\-\.]+/i)` with `format: false`
  (`config/routes.rb:105-107`); the deployment `domains/:domain` member uses
  `/[^\/]+/` (`config/routes.rb:156`). The org path segment accepts only
  letters, digits, `-` and `.` — an underscore (or anything else) yields a 404
  before the controller runs, and `format: false` is what keeps `.com` from
  being parsed as a response format.
- **Two records, no shortcuts.** `verify` needs the TXT (ownership) *and* the
  CNAME (routing) to reach `verified`; partial success is 200 and reads as done
  if you don't check `data.status`.
- **Token is one-shot-ish.** Only `create` returns `verification_token`; `show`
  and `list` omit it. Re-POSTing a pending domain rewrites the token, so the
  TXT value you already published goes stale (`app/models/custom_domain_verification.rb:28-34`).
- **Lazy verification.** A deployment domain is stored `pending` and is *not*
  pushed to Coolify until the org record is `verified` — `fqdn_sync_string`
  filters on `.verified` (`app/models/github_repo_deployment.rb:274-276`). Adding without
  verifying never serves.
- **One record per org+domain.** `CustomDomainVerification` is unique on
  `(organization_pid, domain)` (`app/models/custom_domain_verification.rb:14`), so the org
  library and a deployment attachment share the same row. Re-adding the same
  domain to a deployment succeeds idempotently (re-affirming primary) instead
  of raising the uniqueness error — the add upserts by domain
  (`app/controllers/api/v1/platform/github_repo_deployments_controller.rb:1118`).
- **Create-time rejections.** `DomainInputValidator` 422s a bare port, an IPv4
  literal, a platform-owned zone, or a provider-owned suffix
  (`*.up.railway.app`, `herokuapp.com`, `vercel.app`, …)
  (`app/services/domain_input_validator.rb:22-46`). `verify` re-checks: a platform-owned
  hostname returns success with `platform_managed: true`; a provider-owned one
  422s with `status: "unsupported_domain"` (`app/controllers/api/v1/platform/custom_domains_controller.rb:74-92`).
- **CNAME targets differ by surface.** Org `create`/`show` use the first
  deployment's `deploy_url` host (or `your-app.selfhost.dev`); the deployment
  response uses that deployment's `system_domain` (`app/controllers/api/v1/platform/github_repo_deployments_controller.rb:1160`).
  Both are valid targets, but don't assume they match.
- **Service applier rejects platform zones.** `.selfhost.dev`, `.sslip.io`,
  `.nip.io`, `.swatantra.cloud` are refused on the service route, and the
  hostname must match its own 253-char format (`app/services/service_provisioning/custom_domain_applier.rb:22-40`).
- **Service `pending` means not applied yet.** A create-time `custom_domain`
  stays `pending` until the service is ready; only then does
  `ProvisionServiceJob` pin it and flip `verifying`
  (`app/jobs/provision_service_job.rb:155-172`). `POST /custom_domain` is the
  retry affordance after a `failed` verification.
- **Attach before serving.** The org library only proves ownership/routing.
  Without an attachment there is no Coolify FQDN change, hence no certificate.
- **Read the deployment domain list, don't mutate it.** `GET
  /deployments/:id` (or `/github_repo_deployments/:id`) returns the deployment
  plus `domains[].{domain,is_primary,status,url,dns_records}` — the only
  non-mutating way to see a domain's `status` and `dns_records`
  (`app/controllers/api/v1/platform/github_repo_deployments_controller.rb:1038,1146-1157`).
  The reason to avoid the mutating routes for a read: `POST .../custom_domain`
  appends the domain and promotes it to primary
  (`github_repo_deployments_controller.rb:531-547`), and `DELETE .../custom_domain`
  removes the whole set (`github_repo_deployments_controller.rb:552-584`).
- **Unprovisioned deployments 422.** Both add and remove require
  `coolify_app_uuid` (`app/controllers/api/v1/platform/github_repo_deployments_controller.rb:1065,553`).

## Sources

- Routes: `config/routes.rb:73-74` (service domain), `:103-108` (org library),
  `:127-132` and `:153-158` (deployment domain members).
- Controllers: `app/controllers/api/v1/platform/custom_domains_controller.rb`,
  `.../github_repo_deployments_controller.rb`, `.../services_controller.rb`.
- Models: `app/models/custom_domain_verification.rb`,
  `app/models/deployment_domain.rb`, `app/models/github_repo_deployment.rb`,
  `app/models/coolify_service.rb`.
- Services/jobs: `app/services/domain_input_validator.rb`,
  `app/services/service_provisioning/custom_domain_applier.rb`,
  `app/jobs/coolify/service_domain_verification_job.rb`.
- CLI verbs: `src/cli/domain.rs`, `src/cli/deploy.rs:130-141`,
  `src/cli/project.rs:63-70`.
