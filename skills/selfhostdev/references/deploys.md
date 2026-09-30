# Deployments (GitHub-backed)

Deploy a GitHub repository onto an existing Coolify *project server* and manage
its build config, runs, env vars, custom domains and notification bindings.
Every `selfhost deploy ...` verb — `create`, `list`, `show`, `update`, `delete`,
`trigger`, `abort`, `health`, and the `runs`/`env`/`domain`/`notify`/
`build-config` groups — answers `not implemented yet: deploy ...` today, so use
`selfhost api` with the paths below. A deployment is a `deploy_<ulid>` pid,
scoped to one organization and one `coolify_project` pid (`prj_…`), which is the
"environment" it runs in. All routes below are org-scoped.

## Before you start

- `selfhost auth status` must pass. Every action runs `authenticate_user!` and
  resolves `Organization.find_by(pid: params[:organization_id])`
  (`…github_repo_deployments_controller.rb:8,615`), so `organization_id` must
  be an organization **pid** — the CLI resolves a slug and injects it
  ([api.md](api.md#the-org-placeholder)).
- Roles (`…:11-13`): `create` needs `projects:create`; `update`, `deploy`,
  `abort`, `build_config`, domain changes and every env-var/notification write
  need `projects:update`; `delete` needs `projects:destroy`.
- The environment (`coolify_project_pid`) must exist and be deployable — get one
  from `selfhost project ...` ([services.md](services.md)).
- Calling rules: a GET with parameters needs `-X GET` (a bare `-f` sends a
  POST); nested bodies (`env_vars`, `public_ports`, `domains`, `ignore_paths`)
  need `--input -` (no nested-parameter syntax, and it keeps secrets out of
  your shell history); `api` never prompts and refuses `--dry-run`
  ([api.md](api.md), [commands.md](commands.md)).
- Use the canonical `github_repo_deployments` path; the `/deployments` alias is
  broken for sub-resources (Gotchas).

## Endpoint map

Short names — all under `app/controllers/api/v1/platform/`: `C` =
`…github_repo_deployments_controller.rb`, `R` = `…github_repo_deployment_runs_controller.rb`,
`E` = `…github_repo_deployment_env_vars_controller.rb`, `N` =
`…github_repo_deployment_notification_bindings_controller.rb`.

### Lifecycle — `/api/v1/platform/github_repo_deployments`

| Future CLI command | Method and path | What it does | Key parameters | Source |
| --- | --- | --- | --- | --- |
| `deploy list` | `GET /api/v1/platform/github_repo_deployments` | Org's deployments, newest first, each with its 50 newest runs | `organization_id` (injected); optional `is_preview` | `config/routes.rb:120`, `C:18-32` |
| `deploy show` | `GET …/github_repo_deployments/:id` | One deployment + latest runs + domains | `id` = `deploy_…` pid | `config/routes.rb:120`, `C:35-40`, `C:1042` |
| `deploy create` | `POST …/github_repo_deployments` | Declare a deployment and enqueue provisioning (async, 201) | see body below | `config/routes.rb:120`, `C:43-274` |
| `deploy update` | `PATCH …/github_repo_deployments/:id` | Pause/resume, `auto_deploy`, `ignore_paths`, preview cap, or replace the domain set | `id`; body below | `config/routes.rb:120`, `C:278-311`, `C:1054` |
| `deploy delete` | `DELETE …/github_repo_deployments/:id` | Destructive: removes the Coolify app, then soft-deletes the row | `id` | `config/routes.rb:120`, `C:587-602` |
| `deploy trigger` | `POST …/github_repo_deployments/:id/deploy` | Starts a `manual` run on the current branch head | `id` | `config/routes.rb:122`, `C:314-353` |
| `deploy abort` | `POST …/github_repo_deployments/:id/abort` | Destructive: cancels the in-progress run | `id` | `config/routes.rb:123`, `C:359-383` |
| `deploy health` | `GET …/github_repo_deployments/:id/health` | Last stored health probe + `deploy_url` | `id` | `config/routes.rb:125`, `C:520-528` |
| `deploy build-config set` | `PATCH …/github_repo_deployments/:id/build_config` | Partial update of build settings (+ optional env/public-port replace) | body below | `config/routes.rb:124`, `C:387-500` |
| `deploy build-config get` | — no GET route — read `build_pack`, `base_directory`, `port`, `public_ports` from `show` | | | `C:1042-1050` |

Response shape of `show` and `index` (`C:1031-1052`): both go through the same
helper, so both return `active_preview_count` plus `pid`, `repo_full_name`,
`branch`, `environment`, `build_pack`, `base_directory`, `port`, `public_ports`,
`public_endpoints`, `custom_domain`, `deploy_url`, `status`, `auto_deploy`,
`pr_number`, `max_concurrent_previews`, `last_deploy_status`, `last_deploy_at`,
`env_redeploy_pending`, `repo_url`, `domains[]` (`domain`, `is_primary`,
`status`, `url`, `dns_records`), `deployment_runs[]` (50 newest).

### Runs

| Future CLI command | Method and path | What it does | Key parameters | Source |
| --- | --- | --- | --- | --- |
| `deploy runs list` | — no list route — `show` returns `deployment_runs[]` (50 newest) | | | `C:1047-1050` |
| `deploy runs logs` | `GET …/:github_repo_deployment_id/runs/:run_id/logs` | Build + container logs as one array, each entry tagged `phase: build\|container` | `run_id` = `run_…` | `config/routes.rb:133`, `R:20-76` |
| `deploy runs redeploy` | `POST …/:github_repo_deployment_id/runs/:run_id/redeploy` | New run from the same commit (branch head if the run has no SHA) | `run_id` | `config/routes.rb:134`, `R:78-80,114-158` |
| `deploy runs rollback` | `POST …/:github_repo_deployment_id/runs/:run_id/rollback` | New run pinned to that run's `commit_sha`; the run must be `active` | `run_id` | `config/routes.rb:135`, `R:83-85,114-158` |

A run payload (`app/models/github_repo_deployment_run.rb:33-50`): `pid`,
`source`, `source_run_pid`, `status`, `commit_sha`, `coolify_deployment_uuid`,
`error_message`, `started_at`, `finished_at`, `deploy_url`, `build_logs`,
`container_logs`. Sources: `manual`, `github_push`, `redeploy`, `rollback`,
`env_var_change`, `create`, `toolchain_refresh`; terminal statuses `active`,
`failed`, `removed`, `aborted`, `skipped` (`…run.rb:6-8`).

### Env vars — nested pid is `github_repo_deployment_id`

| Future CLI command | Method and path | What it does | Key parameters | Source |
| --- | --- | --- | --- | --- |
| `deploy env list` | `GET …/:github_repo_deployment_id/env_vars` | All vars, `key` + plaintext `value`, sorted by key | — | `config/routes.rb:137`, `E:45-50` |
| `deploy env set` | `PUT …/env_vars` | **Full replace**: upserts the body's keys, deletes every other key | `env_vars` object | `config/routes.rb:138`, `E:56-93` |
| `deploy env merge` | `PATCH …/env_vars` | **Upsert**: writes the body's keys, keeps the rest, no deletes | `env_vars` object | `config/routes.rb:139`, `E:98-122` |
| `deploy env unset` | `DELETE …/env_vars/:key` | Destructive: removes one key | `key` | `config/routes.rb:140`, `E:127-142` |
| `deploy env detect` | `GET …/env_vars/detect` | Scans repo files for candidate keys: example env files, workflow secrets, CI configs, Docker files and framework config files | — | `config/routes.rb:136`, `E:17-42` |

Both writes read one key: `env_vars`, a flat object of name → value
(`E:164-169`). Body exactly as the controller builds it:

```sh
# PATCH (merge) — keys absent from the body stay; PUT replaces the whole set
printf '%s' '{"env_vars":{"DATABASE_URL":"postgres://…"}}' \
  | selfhost api -X PATCH /api/v1/platform/github_repo_deployments/deploy_01h…/env_vars --input -
```

Each write syncs the full set to Coolify and then tries to redeploy; the
response carries `env_vars[]`, `redeploy_triggered`, `redeploy_pending`
(`E:66-92,210-275`).

### Domains

| Future CLI command | Method and path | What it does | Key parameters | Source |
| --- | --- | --- | --- | --- |
| `deploy domain list` | — no route — `show` returns `domains[]` with `dns_records` | | | `C:1146-1158` |
| `deploy domain add` | `POST …/:id/custom_domain` (also `POST …/:id/domains`) | Appends one domain, made primary; unverified domains are stored `pending` | `domain` | `config/routes.rb:127-128`, `C:531-549` |
| `deploy domain remove` | `DELETE …/:id/domains/:domain` | Destructive: removes one domain; `DELETE …/:id/custom_domain` removes one domain when given a domain, otherwise removes all | `domain` | `config/routes.rb:130-132`, `C:552-584` |
| `deploy domain sync` | `PATCH …/:id/domains` | Replaces the whole domain set with the posted array (only verified ones reach Coolify) | `domains` array | `config/routes.rb:129`, `C:278-281,1064-1144` |
| `deploy domain verify` | `POST /api/v1/platform/custom_domains/:domain/verify` | Org-level DNS check (TXT + CNAME), returns `checks` | `domain` | `config/routes.rb:108`, `…custom_domains_controller.rb:72-137` |

### Notifications

| Future CLI command | Method and path | What it does | Key parameters | Source |
| --- | --- | --- | --- | --- |
| `deploy notify list` | `GET …/:github_repo_deployment_id/notifications` | Channels bound to this deployment | — | `config/routes.rb:141`, `N:23-35` |
| `deploy notify bind` | `POST …/notifications` | Binds an org channel (`nchan_…`, currently `email` only) | `notification_channel_pid` | `config/routes.rb:142`, `N:37-63` |
| `deploy notify unbind` | `DELETE …/notifications/:channel_pid` | Destructive: removes the binding | `channel_pid` | `config/routes.rb:143`, `N:65-82` |

### `api/v2` — declared, not implemented

`config/routes.rb:188-191` declares `POST /api/v2/deployments` and
`POST /api/v2/deployments/preflight`; no controller exists
(`app/controllers/api/v2/` is absent, no `Api::V2` constant anywhere), so
neither path is callable.

## Create one and get it running end to end

1. Resolve the org and the environment pid (`prj_…`):
   `selfhost auth status && selfhost org use acme`; list projects with
   `selfhost api /api/v1/platform/projects -o json` ([services.md](services.md)).

2. Create it. `repo_url` is a repo URL or a `.../tree/<branch>/<dir>` URL; the
   body is flat, or nested under `github_repo_deployment` (`C:46-217,801-820`).
   Required: `repo_url`, `coolify_project_pid` (a wrong pid is 404 "Deployment
   environment not found or not accessible"), and — for a private repo —
   `installation_id` from an active installation (`C:104-113`). Optional:
   `branch`, `environment`, `build_pack` (`nixpacks|dockerfile|dockercompose|static`),
   `base_directory`, `dockerfile_location`, `port`, `public_ports` (`{host_port,
   container_port}`, TCP, max 10), `custom_domain`, `env_vars`,
   `required_env_vars` (each must be present in `env_vars`). Unset `build_pack`/`port`
   are auto-detected (`C:170-198`). It consumes a slot and RAM on the project server
   (`ServiceCapacityGuard`, `C:128`), but is **not a billing call**.

   ```sh
   cat > deploy.json <<'JSON'
   { "repo_url": "https://github.com/acme/web",
     "coolify_project_pid": "prj_01h…",
     "branch": "main",
     "env_vars": { "DATABASE_URL": "postgres://…" },
     "required_env_vars": ["DATABASE_URL"] }
   JSON
   selfhost api -X POST /api/v1/platform/github_repo_deployments \
     --input deploy.json -o json
   ```

3. The 201 is the *start*: `data.deployment.status` is `pending` or
   `provisioning`, `data.deployment_run.status` is `enqueued`, and
   `coolify_app_uuid` is `null` until provisioning finishes (`C:223-274`).
   `deploy_url` is usually already filled in — it falls back to an automatic
   address built from the project server as soon as that server has a public
   IP, so it is only `null` when the project has none yet
   (`C:1036`, `…deployment.rb:290-292`). Poll `show` every ~15-30 s.

   ```sh
   selfhost api /api/v1/platform/github_repo_deployments/deploy_01h… -o json
   # data.deployment.status                    → active = serving | failed
   # data.deployment.deployment_runs[0].status → enqueued→building→deploying→active
   ```

   Provisioning runs off-request: `Coolify::ProvisionAppJob` waits for the host,
   registers the Coolify app, then triggers the first deploy;
   `Coolify::TriggerDeployJob` calls Coolify and `Coolify::ReconcileDeployRunJob`
   advances the run every 30 s (`config/recurring.yml:179`,
   `app/jobs/coolify/reconcile_deploy_run_job.rb:33-42`). A build past 30 min,
   or a container not up within 5, is aborted.

4. On success, read the URL, health and the first logs. Health is refreshed in
   the background every minute (`config/recurring.yml:163`), so it can lag the
   run.

   ```sh
   selfhost api /api/v1/platform/github_repo_deployments/deploy_01h…/health -o json
   selfhost api /api/v1/platform/github_repo_deployments/deploy_01h…/runs/run_01h…/logs -o json
   ```

5. Later deploys: `POST …/:id/deploy` (no body) starts a `manual` run from the
   branch head; a push to a tracked branch does it when `auto_deploy` is true.

## Day-2 operations

| Operation | Call |
| --- | --- |
| Pause / resume | `-X PATCH …/deploy_01h… -f status=paused` (or `active`) |
| Other settings | same PATCH: `-f auto_deploy=false`, `-f max_concurrent_previews=3` (1-20), or `--input -` for `ignore_paths` |
| Trigger / abort | `-X POST …/:id/deploy` / `…/:id/abort` |
| Health snapshot | `GET …/:id/health` — `status`, `reachable`, `checked_at`, `error`, `deploy_url`, `domains` |

## Gotchas

- **The `/deployments` alias breaks sub-resources.** `config/routes.rb:146-170`
  mounts the same controller at `/api/v1/platform/deployments`. Its lifecycle
  member routes carry the pid as `:id` (read by `C`), but its `runs/…`,
  `env_vars` and `notifications` children carry it as `:deployment_id` while
  those controllers read `params[:github_repo_deployment_id]`
  (`E:156-161`, `R:101-107`, `N:95-98`) — they 404. Use the
  `github_repo_deployments` path everywhere.
- **Two segment names, one pid.** Nested routes read
  `:github_repo_deployment_id`; member routes (`deploy`, `abort`, `build_config`,
  `health`, `custom_domain`, `domains`, `show`, `update`, `destroy`) read `:id`.
- **Env var values are plaintext on read.** Encrypted at rest
  (`app/models/github_repo_deployment_env_var.rb:8`), but `GET env_vars` and
  `detect` return them, and `build_logs`/`container_logs` can echo them. Never
  paste `api` output. Only `delete` wipes values and redacts old logs
  (`app/models/github_repo_deployment.rb:241-258`); `unset` just removes the
  row, so old build/container logs can still show the value (`E:127-142`).
- **`PUT` env_vars is a full replace** — keys absent from the body are deleted;
  use `PATCH` to add. Blank values are refused on both, and names must match
  `[A-Za-z_][A-Za-z0-9_]*` (`E:171-179`, `…env_var.rb:14-18`).
- **Each env-var write tries to redeploy** (rate-limited to one per 60 s); if
  the deployment is not yet `active` the redeploy is deferred
  (`redeploy_pending: true`) for `EnvRedeployFollowupJob` (`E:202-275`).
- **`build_config` is partial, with two traps.** Absent fields keep their value
  (`C:391-396`); `env_vars` and `public_ports` replace their whole set *only*
  when the key is present. Trap: presence is tested differently — `env_vars`
  counts as present even when sent as JSON `null` (`params.key?`), so
  `{"env_vars":null}` wipes every var; `public_ports` treats `null` as not
  provided, and an explicit `[]` clears it (`C:396-401,455-470,714-722`). A
  public-port change needs an environment that is `active` or `stopped`, and
  `redeploy_required: true` is answered only when the port mappings actually
  changed — re-posting an unchanged set sets no flag, and a plain
  container-`port` change has no gate and never sets it (`C:419-503`).
- **`build-config get` has no route.** `show` exposes `build_pack`,
  `base_directory`, `port`, `public_ports`; `dockerfile_location` is not
  returned (`C:1042-1050`).
- **Trigger guards.** Needs status `active` and a `coolify_app_uuid`; a run in
  flight is **409** on `deploy` but **422** on `runs/:id/redeploy`
  (`C:315-326`, `R:114-123`). A `pending`/`provisioning` deployment still says
  "Deployment is paused" — wait for `active`.
- **Rollback needs a finished run with a recorded commit.** Only a run with
  status `active` (the finished state, `R:83-85`) and a non-blank `commit_sha`
  rolls back. Runs started at creation and runs started by env-var changes
  carry no commit by construction, so rolling those back answers 422 — as does
  a manual run whose GitHub lookup failed (`C:233-258`, `E:210-275`,
  `R:127-134`).
- **Domains are lazily verified.** `add` stores `status: pending`; only
  `verified` domains reach Coolify. `sync` (`PATCH /domains`) replaces the whole
  set, deleting domains you omit (`C:1064-1144`), and `custom_domain`/
  `deploy_url` are written from a *verified* primary only
  (`app/models/github_repo_deployment.rb:274-300`).
- **`delete` is a soft delete** (`deleted_at`); the row and runs stay for audit
  and the repo@branch slot is freed (`app/models/github_repo_deployment.rb:42`).
- **`GET …/:id/logs` does not exist** (the controller's application-`logs`
  action is unrouted); use `runs/:run_id/logs`.
- **`notification_channel_pid`** must be an org channel (`nchan_…`); an
  already-bound channel is 422 (`N:37-63`).

## Sources

- `config/routes.rb:103-110` (org custom domains + verify), `:120-144`
  (canonical deployments), `:146-170` (alias), `:188-191` (`api/v2`).
- `app/controllers/api/v1/platform/github_repo_deployments_controller.rb`:
  `:8-14,18-40,43-274,278-311,314-383,387-500,520-602,801-820,1031-1062,1064-1158`.
- `…github_repo_deployment_runs_controller.rb:20-158`;
  `…github_repo_deployment_env_vars_controller.rb:17-142,156-179,202-288`;
  `…github_repo_deployment_notification_bindings_controller.rb:23-109`;
  `…custom_domains_controller.rb:72-137`.
- `app/models/github_repo_deployment.rb:14-15,42,241-258,264-333,346-355`;
  `…github_repo_deployment_run.rb:6-8,33-50`;
  `…github_repo_deployment_env_var.rb:8-25`; `deployment_domain.rb:4-33`;
  `notification_channel_binding.rb:22-28`; `notification_channel.rb:6,32`.
- `app/jobs/coolify/{provision_app,trigger_deploy,reconcile_deploy_run}_job.rb`,
  `app/jobs/github_repo_deployment_health_check_job.rb`;
  `config/recurring.yml:163,179`.
