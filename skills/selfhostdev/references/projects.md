# Projects

A project is one Coolify VM — the server every database, service and deployment
in the `selfhost project` group runs on. The typed commands (`selfhost project
list|show|create|update|delete|metrics|activities`,
`project db|service|backup|snapshot|ssh …`) answer `not implemented yet`, so
drive the platform API with `selfhost api` and the paths below. A project's pid
is `prj_` + ULID (`app/models/coolify_project.rb:326`); its children have their
own pids (`prj_pg_…`, `prj_svc_…`). No route here carries `{org}`:
scoping is the `organization_id` parameter the CLI injects (see
[api.md](api.md)), read by name at
`app/controllers/api/v1/platform/coolify_projects_controller.rb:346`.

## Before you start

- Signed in: `selfhost auth status` exits 3 if the profile is not usable.
- Organization resolved: `selfhost org use <slug>`, `--org <slug|pid>`, or
  `SELFHOSTDEV_ORG`. Every request on this page is scoped to it through the
  injected `organization_id`; a member of the org (or a platform admin) is
  required — 403 when the caller is not in it, 422 when the pid resolves to no
  organization. Pass `-f organization_id=org_abc123` to override.
- Calling rules from [api.md](api.md), which the examples below assume: default
  method is `GET`; any `-f`/`-F`/`--input` switches to `POST` unless `-X` names
  the method; `-X GET` is what keeps parameters on a GET on the query string;
  `-F` types `true`/`false`/`null`/integers and leaves everything else a string;
  `--input -` sends stdin verbatim as the JSON body.
- There is no nested-parameter syntax: `-f 'project[name]=x'` sends the literal
  key `project[name]` and the server's `params.require(:project)` misses. Bodies
  with a nested `project` object (create, update) must go through `--input -`.
- A POST with no body still needs `-X POST`; without it `api` sends a GET.

## Endpoint map

### The project

| Future CLI command | Method and path | What it does | Key parameters | Source |
| --- | --- | --- | --- | --- |
| `selfhost project list` | `GET /api/v1/platform/projects` | Read-only. Projects in the org in `data.projects` (no pagination). Excludes `deleting`, `terminated` and `restoring` rows. | – | `config/routes.rb:33`, `app/controllers/api/v1/platform/coolify_projects_controller.rb:23` |
| `selfhost project show` | `GET /api/v1/platform/projects/:id` | Read-only. One project in `data.project`; `:id` is the platform pid `prj_…`, not the Coolify uuid. | – | `config/routes.rb:33`, `app/controllers/api/v1/platform/coolify_projects_controller.rb:31` |
| `selfhost project create` | `POST /api/v1/platform/projects` | Mutating and **billable**: starts hourly compute billing. 202, then provisioning runs in the background. | JSON body `project.name`, `project.description`, `project.server_type`, `project.location`, `project.backups_enabled`, `project.ssh_key_pids`, `project.generate_ssh_key`, `project.ssh_key_name` | `config/routes.rb:33`, `app/controllers/api/v1/platform/coolify_projects_controller.rb:38` |
| `selfhost project update` | `PATCH /api/v1/platform/projects/:id` | Mutating. Renames or re-describes; nothing else is writable. | JSON body `project.name`, `project.description` | `config/routes.rb:33`, `app/controllers/api/v1/platform/coolify_projects_controller.rb:183` |
| `selfhost project delete` | `DELETE /api/v1/platform/projects/:id` | Destructive, async. 409 while the project is `pending`/`provisioning`/`deleting` or a deployment run is active; otherwise 202 and status becomes `deleting`. | – | `config/routes.rb:33`, `app/controllers/api/v1/platform/coolify_projects_controller.rb:263` |
| `selfhost project metrics` | `GET /api/v1/platform/projects/:id/metrics` | Read-only. Heartbeat batches in `data.batches`; empty when no agent is installed. | `start_date`, `end_date` (`YYYY-MM-DD`, default last 24 h) | `config/routes.rb:34`, `app/controllers/api/v1/platform/coolify_projects_controller.rb:194` |
| `selfhost project activities` | `GET /api/v1/platform/projects/:project_id/activities` | Read-only. Merged Hetzner server actions (`source: "hetzner"`) and platform events (`source: "platform"`), newest first. Nested reads (databases/services index and show, backups/snapshots index, keys index, access details) also answer while the project is `restoring`/`deleting`; only show/update/destroy/metrics refuse those statuses. | `page` (default 1), `per_page` (default 25, max 50) | `config/routes.rb:36`, `app/controllers/api/v1/platform/project_activities_controller.rb:23`, `app/services/project_activity_feed_service.rb:27` |

### The project's databases

Creation, deletion and container logs/stats live in the engine references:
[`postgresql.md`](postgresql.md), [`mysql.md`](mysql.md),
[`redis.md`](redis.md), [`clickhouse.md`](clickhouse.md),
[`opensearch.md`](opensearch.md) — they document
`POST …/projects/:project_id/databases/:db_type`,
`DELETE …/databases/:pid`, and the async `…/databases/:pid/logs|stats` pairs
(`config/routes.rb:41-46`).

| Future CLI command | Method and path | What it does | Key parameters | Source |
| --- | --- | --- | --- | --- |
| `selfhost project db list` | `GET /api/v1/platform/projects/:project_id/databases` | Read-only. Live (non-deleted) databases in `data.databases`, newest first. No passwords: rows carry the non-secret connection fields (username, database, host, port where the engine provides them); only the password is withheld. | – | `config/routes.rb:37`, `app/controllers/api/v1/platform/databases_controller.rb:30` |
| `selfhost project db show` | `GET /api/v1/platform/projects/:project_id/databases/:pid` | Read-only. One database in `data.database`; refreshes status from Coolify when the server is up, and **includes the password** in `data.database.connection.password`. | `:pid` is the platform database pid | `config/routes.rb:38`, `app/controllers/api/v1/platform/databases_controller.rb:45` |
| `selfhost project db connection` | same as `project db show` | Read-only. There is no separate connection route; the string is `data.database.connection` (host, port, plus username and database name where the engine provides them — Redis rows carry no username or database name — and `password` only here). | – | `config/routes.rb:38`, `app/models/coolify_database.rb:65` |

### The project's services

One-click template creation, deletion, restart, custom domains and container
logs/stats are in [`services.md`](services.md) — they document
`POST …/projects/:project_id/services/:template_type`,
`DELETE …/services/:pid`, `POST …/services/:pid/restart`,
`…/services/:pid/custom_domain` and the async `…/services/:pid/logs|stats`
pairs (`config/routes.rb:53-76`).

| Future CLI command | Method and path | What it does | Key parameters | Source |
| --- | --- | --- | --- | --- |
| `selfhost project service list` | `GET /api/v1/platform/projects/:project_id/services` | Read-only. Services in `data.services`, newest first; no credentials. | – | `config/routes.rb:48`, `app/controllers/api/v1/platform/services_controller.rb:40` |
| `selfhost project service show` | `GET /api/v1/platform/projects/:project_id/services/:pid` | Read-only. One service in `data.service`, including `data.service.credentials` and the custom-domain/TLS fields. | `:pid` is the platform service pid | `config/routes.rb:49`, `app/controllers/api/v1/platform/services_controller.rb:55` |

### Backups (Hetzner-managed, automatic daily)

| Future CLI command | Method and path | What it does | Key parameters | Source |
| --- | --- | --- | --- | --- |
| `selfhost project backup list` | `GET /api/v1/platform/projects/:project_id/backups` | Read-only. `data.backups_enabled`, `data.backup_schedule` (cadence/retention/window), `data.backups` (each with `image_id`). Works on a stopped project. | – | `config/routes.rb:79`, `app/controllers/api/v1/platform/project_backups_controller.rb:28` |
| `selfhost project backup enable` | `POST /api/v1/platform/projects/:project_id/backups/enable` | Mutating and **billable**: adds ~20% of the server rate. Idempotent. 202 with `data.backups_enabled: true`. 409 unless the project is `active`. | – | `config/routes.rb:80`, `app/controllers/api/v1/platform/project_backups_controller.rb:45` |
| `selfhost project backup disable` | `POST /api/v1/platform/projects/:project_id/backups/disable` | Mutating, destructive to data: **Hetzner deletes the existing backups**. Idempotent. 409 unless `active`. | – | `config/routes.rb:81`, `app/controllers/api/v1/platform/project_backups_controller.rb:61` |
| `selfhost project backup restore` | `POST /api/v1/platform/projects/:project_id/backups/restore` | Destructive, async: rebuilds the VM's boot disk from a backup image. 202, project status becomes `restoring`. | `image_id` (from `backup list`), `name` (must equal the project's name or 422) | `config/routes.rb:82`, `app/controllers/api/v1/platform/project_backups_controller.rb:73` |

### Snapshots (manual, on-demand images)

| Future CLI command | Method and path | What it does | Key parameters | Source |
| --- | --- | --- | --- | --- |
| `selfhost project snapshot list` | `GET /api/v1/platform/projects/:project_id/snapshots` | Read-only. `data.snapshots` (each with `image_id`, `description`, `status`, `size_gb`) and `data.max_snapshots` (default 10). | – | `config/routes.rb:86`, `app/controllers/api/v1/platform/project_snapshots_controller.rb:25` |
| `selfhost project snapshot create` | `POST /api/v1/platform/projects/:project_id/snapshots` | Mutating and **billable**: the image bills per GB/month. 202; the image starts as `creating`. 409 unless `active`; 422 ("already has the maximum of 10 snapshots") when at the cap. | `description` (or `name`; truncated to 100 chars) | `config/routes.rb:87`, `app/controllers/api/v1/platform/project_snapshots_controller.rb:36,99` |
| `selfhost project snapshot restore` | `POST /api/v1/platform/projects/:project_id/snapshots/:image_id/restore` | Destructive, async: rebuilds the VM from this project's snapshot. 409 unless the project is `active`; 404 for an image this project does not own, 422 for a name mismatch, else 202 and status `restoring`. | `name` (must equal the project's name) | `config/routes.rb:88`, `app/controllers/api/v1/platform/project_snapshots_controller.rb:53` |
| `selfhost project snapshot delete` | `DELETE /api/v1/platform/projects/:project_id/snapshots/:image_id` | Destructive. 404 for an image this project does not own, else deletes it. 409 unless `active`. | – | `config/routes.rb:89`, `app/controllers/api/v1/platform/project_snapshots_controller.rb:77` |

### SSH keys and SSH access

| Future CLI command | Method and path | What it does | Key parameters | Source |
| --- | --- | --- | --- | --- |
| `selfhost project ssh key list` | `GET /api/v1/platform/projects/:project_id/ssh_keys` | Read-only. `data.ssh_keys`, plus `data.ssh_command` (`ssh shell@<ip>`, null before the server has an IP), `data.source`, `data.cidrs`, `data.requester_ip`. | – | `config/routes.rb:92`, `app/controllers/api/v1/platform/project_ssh_keys_controller.rb:26` |
| `selfhost project ssh key add` | `POST /api/v1/platform/projects/:project_id/ssh_keys` | Mutating. Reference an org-library key by `org_ssh_key_pid`, or inline `name`+`public_key` (also saved to the library). 201. | `org_ssh_key_pid` **or** `name`, `public_key`; optional `access_level` (`read_only` default, `write`), `source`, `cidrs` | `config/routes.rb:93`, `app/controllers/api/v1/platform/project_ssh_keys_controller.rb:44` |
| `selfhost project ssh key remove` | `DELETE /api/v1/platform/projects/:project_id/ssh_keys/:pid` | Destructive. Removes the key from the box (when installed and the agent is up) and deletes the row. 404 if the project has no such key row. | `:pid` is the `prjkey_…` row pid | `config/routes.rb:94`, `app/controllers/api/v1/platform/project_ssh_keys_controller.rb:146` |
| `selfhost project ssh access set` | `PUT /api/v1/platform/projects/:project_id/ssh_access` | Mutating. Sets which source IPs may reach :22. Answers `data.source`, `data.cidrs`, `data.applied`, `data.apply_note`. | `source` (`anywhere`\|`my_ip`\|`custom`), `cidrs` (JSON array, honored only for `custom`) | `config/routes.rb:98`, `app/controllers/api/v1/platform/project_ssh_access_controller.rb:32` |
| – (no verb; used by `set`) | `GET /api/v1/platform/projects/:project_id/ssh_access` | Read-only. Current mode and CIDRs. | – | `config/routes.rb:97`, `app/controllers/api/v1/platform/project_ssh_access_controller.rb:25` |

### Server-type catalogue (needed to pick a create target)

| Future CLI command | Method and path | What it does | Key parameters | Source |
| --- | --- | --- | --- | --- |
| – (feeds `project create`) | `GET /api/v1/platform/server_types` | Read-only, and the one endpoint here that skips authentication. `data.locations[].server_types[]` carries `code`, `memory_gb`, `architecture`, `available` and the per-location hourly price. Filtered to ≥ 4 GB. | – | `config/routes.rb:32`, `app/controllers/api/v1/platform/server_types_controller.rb:16` |

## Provision one end to end

```sh
# 0. profile and organization
selfhost auth status
selfhost org use acme            # or pass --org acme on every call

# 1. pick a size and a location (read-only, unauthenticated).
#    Take a server_types[].code with available:true and memory_gb>=4, and its
#    location code (fsn1, nbg1, hel1, ash, hil, sin).
selfhost api /api/v1/platform/server_types -o json

# 2. create the project. BILLABLE: hourly compute billing starts here
#    (backups_enabled adds ~20% on top). --input - is required because the
#    payload is nested under "project"; 202 means provisioning was queued.
cat <<'JSON' | selfhost api -X POST /api/v1/platform/projects --input - -o json
{"project":{"name":"staging","description":"Team staging box",
            "server_type":"cx23","location":"fsn1",
            "backups_enabled":false,"generate_ssh_key":true}}
JSON
```

Read `data.project.pid` (`prj_…`) from the 202 — every later call needs it. If
the body asked for `generate_ssh_key: true`, `data.generated_ssh_key.private_key`
is present in this response **only**: store it now, it is never returned again.

```sh
# 3. poll until it can run workloads. status: pending -> provisioning -> active;
#    "ready" is true only when the project is active with no error and every
#    child service/database is ready.
selfhost api /api/v1/platform/projects/prj_abc123 -o json
selfhost api -X GET /api/v1/platform/projects/prj_abc123/metrics -F start_date=2026-09-28 -F end_date=2026-09-29

# 4. connection details. SSH first (no child resources needed):
selfhost api /api/v1/platform/projects/prj_abc123/ssh_keys -o json   # data.ssh_command
#    databases come from the engine references, then read one back with its
#    password: data.database.connection
selfhost api /api/v1/platform/projects/prj_abc123/databases -o json

# 5. tear down (destructive, async; 409 while pending/provisioning or a
#    deployment run is active). Status goes deleting -> terminated.
selfhost api -X DELETE /api/v1/platform/projects/prj_abc123
```

Waiting pattern: poll `GET /projects/:id` every ~15 s and watch `status` and
`ready` (same payload). A failure is `status: "failed"` with
`readiness_detail.has_error: true` and no message text. Once the project is
`restoring` or `deleting`, `show` and `index` stop returning it — poll
`GET /projects/:id/activities` instead.

## Day-2 operations

| Task | Call |
| --- | --- |
| Rename / re-describe | `cat <<'JSON' \| selfhost api -X PATCH /api/v1/platform/projects/prj_abc123 --input -` with `{"project":{"name":"staging-2"}}` |
| CPU / memory / disk metrics | `selfhost api -X GET /api/v1/platform/projects/prj_abc123/metrics -F start_date=2026-09-01 -F end_date=2026-09-29` |
| What happened | `selfhost api -X GET /api/v1/platform/projects/prj_abc123/activities -F page=1 -F per_page=25` |
| Enable/disable backups | `selfhost api -X POST /api/v1/platform/projects/prj_abc123/backups/enable`, or `…/backups/disable` |
| Restore from a backup | `selfhost api -X POST /api/v1/platform/projects/prj_abc123/backups/restore -f image_id=1234567 -f name=staging` |
| Snapshot now | `selfhost api -X POST /api/v1/platform/projects/prj_abc123/snapshots -f description=before-upgrade` |
| List / drop snapshots | `selfhost api -X GET /api/v1/platform/projects/prj_abc123/snapshots`, then `selfhost api -X DELETE /api/v1/platform/projects/prj_abc123/snapshots/1234567` |
| Allow SSH from your address | `selfhost api -X PUT /api/v1/platform/projects/prj_abc123/ssh_access -f source=my_ip` |
| Allow SSH from specific ranges | `echo '{"source":"custom","cidrs":["203.0.113.5/32","198.51.100.0/24"]}' \| selfhost api -X PUT /api/v1/platform/projects/prj_abc123/ssh_access --input -` |
| Add a key from the org library | `selfhost api /api/v1/platform/projects/prj_abc123/ssh_keys -f org_ssh_key_pid=orgkey_abc123 -f access_level=read_only` |
| Add a key inline | `selfhost api /api/v1/platform/projects/prj_abc123/ssh_keys -f name=laptop -F public_key=@$HOME/.ssh/id_ed25519.pub` |

`project backup restore` and `project snapshot restore` both take the image id
from their respective list endpoints, both require `name` to equal the project's
name exactly, and both run through the same rebuild job, so the project is
unavailable until it finishes and status returns to `active`.

## Gotchas

- Bodyless POSTs need `-X POST`: `api` sends a GET by default, so
  `selfhost api /api/v1/platform/projects/prj_abc123/backups/enable` alone would
  hit the GET-only route space and fail. Same for `snapshot create`,
  `backup disable`, `snapshot restore`.
- Nested JSON is impossible with `-f`: `project create` needs `params[:project]`
  (`app/controllers/api/v1/platform/coolify_projects_controller.rb:450`) with
  `name`/`description`, so it takes `--input -`. `server_type`, `location`,
  `backups_enabled`, `generate_ssh_key` and `ssh_key_pids` are read outside
  strong params — they still go **inside** the `project` object (the two
  SSH-flag lookups also accept top level).
- `:id` / `:project_id` in every path is the platform pid `prj_…`
  (`CoolifyProject.find_by(pid: …)`, e.g. `…/coolify_projects_controller.rb:351`).
  The Coolify-side `coolify_project_uuid`, the root password/token, root
  email/username, `provisioning_state` and `error_message` are stripped from
  every project payload by `as_json`
  (`app/models/coolify_project.rb:266`). Child pids follow the same rule:
  `databases/:pid` is `prj_pg_…`/`prj_my_…`/etc., while the Coolify call uses the
  child's own `uuid`, which is also stripped.
- A failed project gives you `status: "failed"` and
  `readiness_detail.has_error: true` but no message; the text exists on the row
  and is deliberately hidden from the API.
- `update` writes only `name` and `description`. `server_type` and `location` are
  creation-only — Hetzner cannot move a running server, and both actions share
  one permit list.
- Name uniqueness is a partial index that ignores
  `failed`/`terminated`/`deleting` (`app/models/coolify_project.rb:45-47`), so a name
  frees up the moment a delete starts.
- `delete` is async: it sets `deleting` immediately, the row then disappears from
  `index`/`show` (404), and the job tears down the VM, its backups and every child.
- Backups are Hetzner-native: no local record, 7 rotated daily copies, and
  `enable`/`disable`/`restore` all 409 unless status is exactly `active`.
  `restore` additionally needs `backups_enabled: true`, an `image_id` from
  `backup list`, and `name` equal to the project name, or it is 422.
  `disable` deletes the backups Hetzner holds.
- Snapshot `create` (cap 10 per project, `ServerSnapshotService::MAX_SNAPSHOTS`)
  returns immediately with a `creating` image; poll the list until its `status`
  is `available` before restoring from it. Restore and delete both refuse an
  image the project does not own with a 404, so an id from another project's list
  is never usable.
- `ssh_access` values are exactly `anywhere`, `my_ip`, `custom`
  (`app/models/coolify_project.rb:55`). `cidrs` is honored only for `custom`;
  `my_ip` always uses the address the **server** saw on this request, never a
  client-supplied one. A restricted mode is refused with 422 while the platform's
  own egress IPs cannot be resolved, because saving it would lock the platform
  out of the box. `applied: false` with an `apply_note` means the setting was
  saved but the :22 firewall rule is still being reconciled.
- `ssh key add` with `access_level: "write"` does **not** install the key: the
  row is `pending_approval` until a platform admin approves it, and the response
  message says so. `read_only` (the default) installs immediately when the
  hostlink agent is up, and is queued otherwise — a `status: "queued"` row
  installs itself once the project finishes provisioning. The project cap is 10
  keys, and both levels land on the non-root `shell` user.
- SSH keys added with `ssh_key_pids` on create are installed on the `shell` user
  after provisioning — never on root — and an unknown pid is skipped silently.
- `generate_ssh_key: true` returns the private half once as
  `data.generated_ssh_key.private_key` and never stores it; the public half is
  saved as a platform-generated org key (`orgkey_…`).
- `create` gates in this order: 422 for platform capacity, then for a size below
  4 GB RAM, out of stock everywhere, an unavailable requested
  location, or no SKU for that type+location; only then 402 when the org fails the
  balance or runway check. The location you send is never silently swapped.
  Backups requested on a not-yet-active project are only recorded
  (`backups_requested`) and switched on by the provisioning job (already priced).
- `metrics` (like `show`, `update`, `destroy`) 404s on a `restoring` project — one
  status filter admits only pending/provisioning/active/failed/stopped. With no
  agent installed it answers 200 with empty `batches` and null
  `agent_status`/`last_seen_at`; an unparseable date falls back to the default 24 h
  window and only a reversed range is 422 (`app/controllers/api/v1/platform/coolify_projects_controller.rb:202-206`).
- `project db list` hides soft-deleted rows and never contains a password;
  `project db show` (the `connection` view) is the only place
  `data.database.connection.password` appears.

## Sources

- Routes: `config/routes.rb:32-98` (project block), `config/routes.rb:33` (resources + params), `:34` metrics, `:36` activities, `:37-38` databases, `:48-49` services, `:79-82` backups, `:86-89` snapshots, `:92-94` ssh keys, `:97-98` ssh access.
- Controllers: `app/controllers/api/v1/platform/coolify_projects_controller.rb` (23, 31, 38, 183, 194, 263, 346, 351, 450), `project_activities_controller.rb` (23), `databases_controller.rb` (30, 45), `services_controller.rb` (40, 55), `project_backups_controller.rb` (28, 45, 61, 73), `project_snapshots_controller.rb` (25, 36, 53, 77), `project_ssh_keys_controller.rb` (26, 44, 146), `project_ssh_access_controller.rb` (25, 32), `server_types_controller.rb` (16), `app/controllers/concerns/response_handler.rb` (7), `app/controllers/concerns/billing_gate.rb` (11, 28).
- Models: `app/models/coolify_project.rb` (13, 45-47, 55, 225, 235, 266, 326), `coolify_database.rb` (36, 48, 65, 114, 125), `coolify_service.rb` (249, 313), `coolify_project_ssh_key.rb` (10, 16, 27, 43, 50, 65), `org_ssh_key.rb` (41, 47).
 - Services: `app/services/project_activity_feed_service.rb` (18, 27, 66), `cloud_provider/hetzner/server_backup_service.rb` (25, 36, 49, 64, 94, 107, 122), `cloud_provider/hetzner/server_snapshot_service.rb` (30, 41, 60, 72, 81, 101), `coolify/host/ssh_source.rb` (18, 28, 40, 49), `cloud_provider/hetzner/reference_data_service.rb` (9, 91, 163).
