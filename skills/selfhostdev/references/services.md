# Template services

One-click template stacks (Nextcloud, n8n, Supabase, Mysterium, URnetwork, a Bring-Your-Own
compose, …) that run on a project's single VM. A service pid is `prj_svc_<ulid>`; the parent
project pid is `prj_<ulid>` (`app/models/coolify_service.rb:314`,
`app/models/coolify_project.rb:326`). Every `selfhost project service …` verb answers
`not implemented yet`, so drive these routes with `selfhost api`. Nothing here is org-scoped:
the controller skips the organization filters, so the `organization_id` the CLI injects into the
query/body is ignored (`app/controllers/api/v1/platform/services_controller.rb:18-19`).
Reads are open to project members; create/destroy need `projects:create`/`projects:destroy`,
restart, custom-domain, reauthenticate and port-sources writes need `projects:update`
(`…services_controller.rb:24-26`; port-sources writes via `service_port_sources_controller.rb:24`).

## Before you start

- Sign in (`selfhost auth status`) and resolve an organization (`selfhost org use <slug>`).
- Get the project pid: `selfhost api /api/v1/platform/projects -o json` → `data.projects[].pid`
  (must not be stopped/failed/deleting/terminated/restoring; a pending/provisioning project is
  accepted and children are declared `pending` until the host exists).
- The calling rules — `-X GET` when a GET carries parameters, `-f` raw vs `-F` typed,
  `--input -` for a raw body, exit codes, `{org}`, org injection — are in
  [api.md](api.md). `POST` here is implicit once you pass a field; no `-y` (api never prompts).
- `api` cannot send a multipart upload, so `compose_file` is unreachable from the CLI (see
  Gotchas).

## Endpoint map

Full paths, nested under `/api/v1/platform/projects/:project_id`:

| Future CLI command | Method and path | What it does | Key parameters | Source |
|---|---|---|---|---|
| `project service list` | `GET /api/v1/platform/projects/:project_id/services` | Non-deleted services, newest first; no pagination | – | `config/routes.rb:48`, `services_controller.rb:41` |
| `project service show` | `GET …/services/:pid` | One service, **with** credentials | – | `config/routes.rb:49`, `services_controller.rb:57-60` |
| `project service create` | `POST …/services/:template_type` | Validate, declare the row, queue provisioning → 202 | `name`, `is_public`, `public_port`, `instant_deploy`, `custom_domain`, per-template fields | `config/routes.rb:75`, `services_controller.rb:72-79,107,192,207` |
| `project service delete` | `DELETE …/services/:pid` | Soft-delete (`deleted`, `terminated`) + queue teardown → 202 | `name` — the exact display name | `config/routes.rb:76`, `services_controller.rb:236,247,249` |

Container inspection (read-only; dispatched to the project's hostlink agent — no Coolify API, no
Docker socket). Both are async: POST dispatches, GET polls.

| Future CLI command | Method and path | What it does | Key parameters | Source |
|---|---|---|---|---|
| `project service logs` | `POST …/services/:pid/logs` | Dispatch a log fetch → 202 `pending` | `lines` (default 200, clamped 1–1000) | `config/routes.rb:53`, `container_inspectable.rb:24,33-45`, `coolify/inspection/container_inspector.rb:27-28,110-115` |
| `project service logs` | `GET …/services/:pid/logs` | Settle pending fetches, return the last 20 | – | `config/routes.rb:54`, `coolify_container_fetch.rb:41-45,62-73` |
| `project service stats` | `POST …/services/:pid/stats` | Dispatch a stats fetch → 202 `pending` | – | `config/routes.rb:55`, `container_inspectable.rb:25,33-45` |
| `project service stats` | `GET …/services/:pid/stats` | Same settle-and-return shape | – | `config/routes.rb:56` |

Day-2 (all require an **active** project — no `pending`/`provisioning` host — except the port-sources read and write below, which work on non-active projects too):

| Future CLI command | Method and path | What it does | Key parameters | Source |
|---|---|---|---|---|
| `project service restart` | `POST …/services/:pid/restart` | Coolify restarts the stack; 422 when `uuid` is blank | – | `config/routes.rb:66`, `services_controller.rb:308-311` |
| `project service custom-domain add` | `POST …/services/:pid/custom_domain` | Pins the domain on the web component and redeploys (Traefik + Let's Encrypt) | `domain` | `config/routes.rb:73`, `services_controller.rb:337-342` |
| `project service custom-domain remove` | `DELETE …/services/:pid/custom_domain` | Re-pins the platform URL and clears domain state | – | `config/routes.rb:74`, `services_controller.rb:375` |
| (no verb yet) | `POST …/services/:pid/reauthenticate` | URnetwork only: swap the auth code, keep the node identity volume | `auth_code` | `config/routes.rb:62`, `services_controller.rb:285`, `urnetwork_reauth_service.rb:34-41` |
| (no verb yet) | `POST …/services/:pid/lock-signup` | **DEPRECATED** — always 422 | – | `config/routes.rb:60`, `services_controller.rb:269,425` |
| (no verb yet) | `GET …/services/:pid/port_sources` | Effective per-port source locks + `requester_ip` | – | `config/routes.rb:71`, `service_port_sources_controller.rb:30,127` |
| (no verb yet) | `PUT …/services/:pid/port_sources` | Set the source for each mentioned port | `ports` map (JSON body) | `config/routes.rb:72`, `service_port_sources_controller.rb:38` |

Per-port firewall sources are keyed per published port: `anywhere`, `my_ip` (captured
server-side), `custom` with 1–16 IPs/CIDRs per port (`service_port_sources_controller.rb:26-27`).
Applies to `compose` services and to templates whose strategy declares locked ports — Mysterium's
panel port 4449 (`mysterium_strategy.rb:26,78-80`) and mysterium-gateway's proxy port 3128
(`mysterium_gateway_strategy.rb:26,75-77`). Anything else is refused (`…:179`).

The create form's server types endpoint (outside the project scope, **unauthenticated**,
60 req/min, ETag/`If-None-Match` → 304):

| Future CLI command | Method and path | What it does | Key parameters | Source |
|---|---|---|---|---|
| – (`catalog` verbs are stubs and target other APIs) | `GET /api/v1/platform/server_types` | Hetzner locations with the platform's server types (≥4 GB RAM; every architecture included) | – | `config/routes.rb:32`, `server_types_controller.rb:13,16,19,33` |

## Provision one end to end

```sh
# 1. read-only: find the project (needs an organization resolved)
selfhost api /api/v1/platform/projects -o json      # data.projects[].pid

# 2. discover a valid template_type — there is no catalogue endpoint.
#    A bad type is refused with the full list and creates nothing (422):
selfhost api /api/v1/platform/projects/prj_xxx/services/not-a-template -f name=probe
#    errors[0]: "Supported templates: adguard-home, anythingllm, …, urnetwork, vaultwarden,
#    wireguard, wordpress"

# 3. mutating: declare + provision (202). Fields are the validator's:
#    every template wants `name`; many want dashboard credentials or a public port.
selfhost api /api/v1/platform/projects/prj_xxx/services/nextcloud \
  -f name=files -F is_public=true -F public_port=8443 \
  -f dashboard_username=admin -f dashboard_password='<strong password>'
```

A `custom_domain` at create is validated for format and, once the project has a public IP, for
DNS before anything is spent — set the A record first or add the domain later.

Fields the validators read, beyond `name`: `public_port` (1024–65535, optional — the template's
own default applies — on templates that publish a host port; Mysterium, URnetwork and
mysterium-gateway have no such field and ignore it), `is_public` (default true),
`instant_deploy`, and per-family extras — `dashboard_username`/`dashboard_password` (Nextcloud,
MinIO, Grafana, …, `service_provisioning/validators/nextcloud.rb:20-29`), `auth_code` (urnetwork,
`service_provisioning/validators/urnetwork.rb:27-31`), `panel_source` (mysterium, singular — captured from the
caller's IP or an explicit CIDR, `service_provisioning/validators/mysterium.rb:10-11,22,60-75`), `compose` +
`import_url` (compose only, `services_controller.rb:87-102`). `custom_domain` is not a validator
field: the controller applies it separately before provisioning. A failed field is a 422 whose
`errors` array names it.

**Not billable by itself**: the service rides the project's already-billed box
(`billing_gate.rb:28-31`), but the create still fails 402 when the org cannot fund that box's
minimum runway (`services_controller.rb:159`), 409 on capacity or a cross-workload port clash
(`:148-152,169-187`), and 422 on duplicate service name in the project.

The 202 body is `data.service` — keep `data.service.pid`.

```sh
# 4. wait: poll until status is `active`. pending → creating → active (or failed).
#    provisioning.stage walks initializing → preparing → deploying → starting →
#    verifying → running; provisioning.events carries the live log lines.
selfhost api /api/v1/platform/projects/prj_xxx/services/prj_svc_yyy -o json
#    also read: error_message (when failed), external_url (USE this one),
#    platform_url, tls_status, custom_domain_status

# 5. read-only: the same call carries the credentials — only SHOW includes them
selfhost api /api/v1/platform/projects/prj_xxx/services/prj_svc_yyy -o json
#    data.service.credentials — strategy-specific: username/password/public_url,
#    plus hints (adguard_home_strategy.rb:108-121, bookstack_strategy.rb:76-82)

# 6. read-only: logs are two calls. Dispatch first, then poll.
selfhost api /api/v1/platform/projects/prj_xxx/services/prj_svc_yyy/logs -f lines=500
#    202 → data.fetch.pid, status "pending"
selfhost api /api/v1/platform/projects/prj_xxx/services/prj_svc_yyy/logs
#    data.fetches[0] — newest first: status completed|failed, output, error

# 7. read-only: stats, same pattern. A dispatch with no fields needs -X POST,
#    otherwise the CLI defaults to GET — which here is the POLL, not the dispatch.
selfhost api -X POST /api/v1/platform/projects/prj_xxx/services/prj_svc_yyy/stats
selfhost api /api/v1/platform/projects/prj_xxx/services/prj_svc_yyy/stats

# 8. destructive: tear down. `name` must equal the display name exactly.
selfhost api -X DELETE \
  "/api/v1/platform/projects/prj_xxx/services/prj_svc_yyy?name=files"
```

Poll the fetch with the same `--timeout`/sleep you would use for any async job; a fetch still
`pending` after 3 minutes settles itself to `failed`
("Timed out waiting for the agent result", `coolify_container_fetch.rb:16,78-81`). Ask again
every few seconds — each GET settles and returns the last 20 fetches for that resource+operation.

## Day-2 operations

```sh
# restart (active project only; 422 "Service is not deployed yet" before first deploy).
# Bodyless POST — without -X POST the CLI would send GET and the route would 404.
selfhost api -X POST /api/v1/platform/projects/prj_xxx/services/prj_svc_yyy/restart

# custom domain — the name must ALREADY resolve to the project's public IP,
# else 422 with DNS instructions (custom_domain_applier.rb:54-57)
selfhost api /api/v1/platform/projects/prj_xxx/services/prj_svc_yyy/custom_domain \
  -f domain=files.example.com

# remove it (re-pins the platform-generated domain)
selfhost api -X DELETE /api/v1/platform/projects/prj_xxx/services/prj_svc_yyy/custom_domain

# URnetwork only: swap the auth code without losing the node identity
selfhost api /api/v1/platform/projects/prj_xxx/services/prj_svc_yyy/reauthenticate \
  -f auth_code='<token from ur.io/app>'

# per-port firewall sources: read
selfhost api /api/v1/platform/projects/prj_xxx/services/prj_svc_yyy/port_sources

# write — one JSON body, because the shape is nested (fields cannot express it)
printf '%s' '{"ports":{"4449":{"source":"custom","cidrs":["198.51.100.0/24"]}}}' \
  | selfhost api -X PUT /api/v1/platform/projects/prj_xxx/services/prj_svc_yyy/port_sources --input -
```

## Gotchas

- **`lock-signup` is a deliberate 422.** It stays routed so old console builds get a typed
  refusal, not a 404. Nothing supports sign-up locking (`services_controller.rb:425-432`).
- **No template catalogue endpoint.** `ServiceProvisioning::Strategy.all` globs
  `app/services/service_provisioning/*_strategy.rb` and each class answers `self.supported_type`
  (`strategy.rb:23-37`); a bad `template_type` is the only API-visible list
  (`services_controller.rb:79`). 46 types exist today, from `adguard-home` to `wordpress`.
- **`GET` with parameters turns into a POST.** All the reads above take no parameters, so they
  are safe as-is; add `-X GET` if you ever append one (`api.md`).
- **Dispatching is always a POST.** `logs` and `stats` share one path per method, so a bodyless
  dispatch needs an explicit `-X POST` — `selfhost api …/stats` alone sends GET, which is the
  poll, not the dispatch. Same for `restart` and `lock-signup` (both bodyless POSTs). Anything
  carrying `-f`/`-F` is already a POST and needs no `-X`.
- **Logs/stats `lines`** is clamped to 1–1000, default 200 (`coolify/inspection/container_inspector.rb:27-28`).
  A blank `uuid` (not deployed yet) or a missing hostlink agent fails the fetch.
- **Credentials only on `show`.** `create`, `list` and `delete` return `data.service` without the
  `credentials` key (only `show` passes `include_credentials: true`), and templates whose strategy
  defines no `credential_metadata` never get one at all. Nothing here is one-time — `show`
  re-returns the same credentials on every call. `configuration` — where passwords, SMTP secrets
  and the URnetwork `auth_code` live — is never serialized, so the auth code is write-only.
- **`delete` needs `name`.** Mismatch is 422 `Display name does not match. Expected: …`, and it
  matches case-sensitively. Put it on the query string or the body; both reach `params[:name]`.
- **Day-2 needs an active host.** `restart`, `custom_domain` (+ remove), `reauthenticate` and
  `lock_signup` run `validate_project_active!` and 409 while the project is still provisioning
  (`services_controller.rb:37-38`). Create/destroy work on a pending project instead.
- **Port sources payload.** `PUT` body is exactly `{"ports":{"<port or range>":{"source":…,"cidrs":[…]}}}`;
  each mentioned port is replaced, unmentioned ones keep their state; `anywhere` is the explicit
  unlock. `my_ip` ignores any address you send — it is captured from the request. A saved `my_ip`
  that no longer matches the caller comes back with `"stale": true`, and `applied`/`apply_note`
  report the firewall reconcile (a failed apply never rolls back the saved row).
  For non-compose services only the strategy's locked ports are accepted; a template with none
  is refused outright.
- **`compose_file` is unreachable via `api`.** The controller needs an uploaded object
  (`respond_to?(:read)`); `-F compose_file=@f.yml` sends a string and 422s. Use
  `-F compose=@my-compose.yml` (pasted-YAML path) or `-f import_url=…`.
- **URnetwork readiness is `running:healthy`**, not merely `running` — a rejected auth code
  restart-loops (`urnetwork_strategy.rb:46-48`). Fix it with `reauthenticate`, not a re-create.
- **`server_types` is public** and answers `{"status":"success","data":{"locations":[...]}}` with
  `code`, `city`, `country`, `network_zone`, `server_types[]` (`code`, `name`, `cores`,
  `memory_gb`, `disk_gb`, `cpu_type`, `architecture`, `category`, `offered`, `available`,
  `deprecated`, `deprecation`, `hourly_price_cents`, `display_price`) —
  `reference_data_service.rb:163,206-230`. It is cached upstream and 304s on a matching
  `If-None-Match`.

## Sources

- `config/routes.rb:32-33, 48-76` — server types, the projects resource and every service route.
- `app/controllers/api/v1/platform/services_controller.rb` (whole file: gates, create pipeline,
  restart, custom domain, deprecated lock-signup, reauthenticate).
- `app/controllers/api/v1/platform/service_port_sources_controller.rb:26-27,30,38-90,127-145,179`.
- `app/controllers/api/v1/platform/server_types_controller.rb:13-45`.
- `app/controllers/concerns/container_inspectable.rb`, `app/controllers/concerns/billing_gate.rb:28-31`.
- `app/models/coolify_service.rb:40-67,249-288,314`, `app/models/coolify_container_fetch.rb:16,41-75`,
  `app/models/concerns/coolify_provisioning_progress.rb:171-192`, `app/services/database_status_mapper.rb:7-62`.
- `app/services/service_provisioning/strategy.rb:23-37`, `…/custom_domain_applier.rb:26-57`,
  `…/urnetwork_strategy.rb:26,46-48`, `…/urnetwork_reauth_service.rb:34-41`,
  `app/services/coolify/inspection/container_inspector.rb:27-28,110-115`.
- Validators: `app/services/service_provisioning/validators/nextcloud.rb:20-29`,
  `app/services/service_provisioning/validators/mysterium.rb:22,60-75`, `app/services/service_provisioning/validators/urnetwork.rb:27-31`,
  `app/services/service_provisioning/validators/custom_compose.rb:17-27`,
  `app/services/cloud_provider/hetzner/reference_data_service.rb:163-228`.
