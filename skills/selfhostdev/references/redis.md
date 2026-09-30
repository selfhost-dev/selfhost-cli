# Redis

Redis comes in two unrelated shapes, and both live behind `selfhost api` because every
`selfhost redis …` verb answers `not implemented yet`:

- **Cloud instances** — EC2 instances (`type_of_dbms=redis`, Redis 7.0/7.2, port 6379) under
  `/aws/v1/instances`, owned by an organization. Instance pid: `awsinst_<20 hex>`. This is the
  full-lifecycle family: users, backups, snapshots, replicas, fork, failover.
- **Project databases** — Coolify-managed Redis containers on a project's shared server under
  `/api/v1/platform/projects/:project_id/databases/redis`, scoped to that project, not to an
  organization. Database pid: `prj_rd_<ulid>`.

The two pid namespaces never mix: an `awsinst_…` never appears in a project database route, and
`prj_rd_…` never in `/aws/v1`. The calling rules are in [api.md](api.md).

## Before you start

- Signed-in profile (`selfhost auth status`). Org for the cloud family (`--org <slug|pid>`, or the
  profile's org); project pid for the project family (`selfhost api /api/v1/platform/projects`).
- `selfhost api` defaults to GET; `-f`/`-F`/`--input` turn it into a POST, so a GET carrying
  parameters needs `-X GET`. `-f` sends strings; `-F` turns only `true`/`false`/`null` and whole
  numbers into their JSON types. Everything else — a decimal such as `1.5`, or `True` — arrives as
  a string, and so does any value read from `@file` or stdin.
- **No nested parameters.** `-f key[sub]=…` is a literal key. Nested bodies (`storage: {…}`,
  `backup: {…}`, `database_user: {…}`) require `--input -` with the whole JSON object.
- **Arrays** (`pids`, `allowed_cidr_ranges`) also require `--input -`.
- Cloud routes are org-scoped: the CLI appends `organization_id` (query for GET/`--input`, JSON body
  for `-f`-only writes). The controller honors `organization_id` and its alias `organization_pid`
  (`app/controllers/application_controller.rb:123-124`); a pid that names another org resolves to
  none → 404, never a silent fallback. Project routes ignore `organization_id` entirely — the project
  pid is the scope (`app/controllers/api/v1/platform/databases_controller.rb:9-10`).
- `{org}` is the only CLI placeholder (resolved org pid).

## Endpoint map

### Cloud instances — `/aws/v1/instances`

All rows org-scoped. `:pid` = `awsinst_…`; users member `:id` = database-user pid (`dbu_…`).

| Future CLI command | Method and path | Effect | Key parameters | Source |
|---|---|---|---|---|
| `redis list` | `GET /aws/v1/instances/by_organization` | read | — (groups by `group_id`) | `config/routes.rb:330`, `instances_controller.rb:1307-1343` |
| `redis show` · `wait` · `metrics` | `GET /aws/v1/instances/:pid` | read | — (also `?instance_id=`, `?snapshot_id=`) | `config/routes.rb:346`, `instances_controller.rb:1221-1304` |
| `redis create` | `POST /aws/v1/instances` | **billable** | `type_of_dbms`, `region`, `identifier`, `instance_type`, `storage` | `config/routes.rb:328`, `instances_controller.rb:108,646` |
| `redis update` | `PATCH /aws/v1/instances/:id` | mutating | `identifier`, `tags`, `public_access`, `delete_protection`, `backup_enabled` | `config/routes.rb:336`, `instances_controller.rb:2045,2070-2105` |
| `redis start` | `PUT /aws/v1/instances/start` | mutating | `pids` (array) | `config/routes.rb:334`, `instances_controller.rb:1901` |
| `redis stop` | `PUT /aws/v1/instances/stop` | mutating | `pids` | `config/routes.rb:333`, `instances_controller.rb:1822` |
| `redis reboot` | `PUT /aws/v1/instances/reboot` | mutating | `pids` | `config/routes.rb:335`, `instances_controller.rb:1990` |
| `redis delete` | `POST /aws/v1/instances/delete` | **destructive** | `pids`, `create_snapshot` | `config/routes.rb:331`, `instances_controller.rb:1346-1415` |
| `redis fork` | `POST /aws/v1/instances/:id/fork` | **billable** | `instance_name` (required), `cloud_credential_id` | `config/routes.rb:337`, `instances_controller.rb:2845,2868-2878` |
| `redis resize` | `PUT /aws/v1/instances/:id/scale` | **billable** | `target_instance_type`, `new_size_gb`, `new_iops`, `new_throughput` | `config/routes.rb:338`, `instances_controller.rb:3151-3161` |
| `redis scale` | `PATCH /aws/v1/instances/:id` | **billable** | `multi_az` (bool) + `replica_count` | `config/routes.rb:336`, `instances_controller.rb:2399,2453-2527` |
| `redis failover` | `POST /aws/v1/instances/:id/trigger_failover` | mutating | `rpo_override` (bool, optional) | `config/routes.rb:339`, `instances_controller.rb:3411,3492-3522` |
| — (Redis unsupported) | `GET /aws/v1/instances/:id/dashboards` | read | — 422 unless `type_of_dbms == "opensearch"` | `config/routes.rb:340`, `instances_controller.rb:3387-3390` |
| `redis list` (refresh) | `POST /aws/v1/instances/refresh_by_group_id` | mutating | `group_id` | `config/routes.rb:332`, `instances_controller.rb:3363-3371` |
| — | `GET /aws/v1/regions` | read | — | `config/routes.rb:435`, `regions_controller.rb:21-26` |
| — | `GET /aws/v1/regions/:region_code/instance_types` | read | `family`, `current_gen`, `min_vcpu`, `min_memory` | `config/routes.rb:436`, `regions_controller.rb:28-45` |

### Pricing — `/aws/v1/pricing`

Two reads that need neither a session nor an organization (`pricing_controller.rb:6-8` skips both),
shared rate limit 60 requests/minute (`:10-11`). Neither bills nor mutates. Field-by-field
coverage lives in [catalog.md](catalog.md).

| Future CLI command | Method and path | Effect | Key parameters | Source |
|---|---|---|---|---|
| — | `GET /aws/v1/pricing` | read | `region` (default `us-east-1`) | `config/routes.rb:444`, `pricing_controller.rb:13-20` |
| — | `POST /aws/v1/pricing/estimate` | read | `region`, `instance_type`, `mode`, `db_type` (`redis`), `storage_type`, `storage_size_gb`, `database_count`, `replica_count`, `public_ipv4_count`, `iops`, `throughput_mbps` | `config/routes.rb:445`, `pricing_controller.rb:22-49` |

Quote a shape before you create it: the estimate creates nothing, and an unknown region, mode,
instance type or storage type — or a size/IOPS/throughput outside what the storage type offers —
comes back 422 (`pricing_controller.rb:29-30`).

### Database users — `/aws/v1/instances/:instance_id/database_users`

Redis ACL users. Backed by `script/redis_user_management.sh` (create/update/delete only — no
engine-side list; `list` reads platform rows). Paid plan required on every mutation.

| Future CLI command | Method and path | Key parameters | Source |
|---|---|---|---|
| `redis users list` | `GET /aws/v1/instances/:instance_id/database_users` | — | `config/routes.rb:348`, `database_users_controller.rb:27-32` |
| `redis users create` | `POST /aws/v1/instances/:instance_id/database_users` | `database_user`: `username`, `password`, `role` | `config/routes.rb:348`, `database_users_controller.rb:36-78,264` |
| `redis users update` | `PATCH /aws/v1/instances/:instance_id/database_users/:id` | `database_user`: `role`, `connection_limit`, `expires_at`, `password_rotation_enabled` | `config/routes.rb:348`, `database_users_controller.rb:19,89-124,264` |
| `redis users delete` | `DELETE /aws/v1/instances/:instance_id/database_users/:id` | — | `config/routes.rb:348`, `database_users_controller.rb:167-204` |
| `redis users rotate-password` | `POST /aws/v1/instances/:instance_id/database_users/:id/rotate_password` | — | `config/routes.rb:349`, `database_users_controller.rb:132-163` |

### Logs and stats — `/aws/v1/instances/:instance_id`

Redis supports neither (adapter inherits `supports_log_tail? = false`,
`supports_db_stats? = false`). Dispatch (`POST`) returns 422; the `logs`/`db_stats` index reads and
`logs/sources` still answer, but with an empty fetch list and `{ sources: [] }`.

| Future CLI command | Method and path | Result for Redis | Source |
|---|---|---|---|
| `redis logs` | `GET /aws/v1/instances/:instance_id/logs` (poll) | 200 with the recent-fetch list (empty for Redis) | `config/routes.rb:362`, `logs_controller.rb:28-36` |
| `redis logs` | `POST /aws/v1/instances/:instance_id/logs` (dispatch) | 422 `Log fetching is not supported for redis instances` | `config/routes.rb:362`, `logs_controller.rb:43-45` |
| — | `GET /aws/v1/instances/:instance_id/logs/sources` | `{ sources: [] }` | `config/routes.rb:363`, `logs_controller.rb:18-20` |
| `redis stats` | `GET /aws/v1/instances/:instance_id/db_stats` (poll) | 200 with the recent-fetch list (empty for Redis) | `config/routes.rb:366`, `db_stats_controller.rb:21-28` |
| `redis stats` | `POST /aws/v1/instances/:instance_id/db_stats` (dispatch) | 422 `Query stats are not supported for redis instances` | `config/routes.rb:366`, `db_stats_controller.rb:31-33` |

### Backups, snapshots, volumes, policies — `/aws/v1`

Redis backups are EBS/DLM only (`supports_ebs_backup? = true`, `supports_native_backup? = false`).
`POST /aws/v1/backups` accepts `aws_backup_job`; the native types are ClickHouse/Kafka only.

| Future CLI command | Method and path | Effect | Key parameters | Source |
|---|---|---|---|---|
| `redis backups list` | `GET /aws/v1/backups` | read | `organization_id` (injected) | `config/routes.rb:413`, `backups_controller.rb:22-36` |
| `redis backups create` | `POST /aws/v1/backups` | **billable** | `backup`: `backup_type=aws_backup_job`, `resource_id` (instance pid), `region`, `options` | `config/routes.rb:413`, `backups_controller.rb:15,54-60,89` |
| `redis backups delete` | `DELETE /aws/v1/backups/:id` | destructive | `:id` = backup pid (`back_…`) | `config/routes.rb:413`, `backups_controller.rb:538-578` |
| `redis backups restore` | `POST /aws/v1/backups/:id/restore` | **422 for Redis** | native types only | `config/routes.rb:416`, `backups_controller.rb:306-310` |
| `redis snapshots create` | `POST /aws/v1/volumes/create_snapshot` | **billable** | `pid` (instance), `volume_type` (default `data`); `organization_id` required (injected) | `config/routes.rb:421`, `volumes_controller.rb:13-28,123-126` |
| `redis snapshots list` | `GET /aws/v1/snapshots` | read | `region`, `cloud_credential_id` | `config/routes.rb:425`, `snapshots_controller.rb:12-33` |
| — | `GET /aws/v1/snapshots/:id` | read | `:id` = AWS `snap-…` | `config/routes.rb:425`, `snapshots_controller.rb:49-86` |
| `redis snapshots delete` | `DELETE /aws/v1/snapshots/:id` | destructive | `:id` = `snap-…` | `config/routes.rb:425`, `snapshots_controller.rb:103-140` |
| — | `POST/GET/PATCH/DELETE /aws/v1/backup_policies[/:id]` | mutating | DLM `EBS_SNAPSHOT_MANAGEMENT` policy | `config/routes.rb:427`, `backup_policies_controller.rb:253-256,279` |

Snapshots are engine-specific: restore a Redis snapshot by **creating a new instance** with
`snapshot_id` (`instances_controller.rb:577-611`), not through `backups/:id/restore`.

### Project databases — `/api/v1/platform/projects/:project_id/databases`

`db_type = redis`. Responses use `CoolifyDatabase#as_json` (no serializer). Status is mapped:
pending/creating/active/inactive/failed/terminated.

| Future CLI command | Method and path | Effect | Key parameters | Source |
|---|---|---|---|---|
| `redis list` | `GET /api/v1/platform/projects/:project_id/databases` | read | — (returns **all** engine types) | `config/routes.rb:37`, `databases_controller.rb:30-36` |
| `redis show` · `wait` | `GET /api/v1/platform/projects/:project_id/databases/:pid` | read | — adds `connection.password` | `config/routes.rb:38`, `databases_controller.rb:44-76` |
| `redis create` | `POST /api/v1/platform/projects/:project_id/databases/redis` | mutating | `name`, `redis_password` | `config/routes.rb:45`, `databases_controller.rb:88-153` |
| `redis delete` | `DELETE /api/v1/platform/projects/:project_id/databases/:pid` | destructive | `name` must equal the display name | `config/routes.rb:46`, `databases_controller.rb:167-188` |
| `redis logs` | `POST …/databases/:pid/logs` (dispatch) · `GET …` (poll) | read | `lines` (default 200, max 1000) | `config/routes.rb:41-42`, `container_inspectable.rb:24-29` |
| `redis stats` | `POST …/databases/:pid/stats` (dispatch) · `GET …` (poll) | read | — | `config/routes.rb:43-44`, `container_inspectable.rb:24-29` |
| `redis metrics` | `GET /api/v1/platform/projects/:id/metrics` | read | `start_date`, `end_date` | `config/routes.rb:34`, `coolify_projects_controller.rb:194-212` |

## Provision one end to end

Cloud instance (org-scoped, billable):

```sh
# 1. read-only: pick a region and an instance type offered there
selfhost api /aws/v1/regions -o json
selfhost api -X GET /aws/v1/regions/eu-west-1/instance_types -f min_vcpu=2 -f min_memory=4096

# 2. billable: create. Nested `storage` forces a raw JSON body (-X not needed: --input is a write).
selfhost api /aws/v1/instances --org acme --input - <<'JSON'
{
  "type_of_dbms": "redis",
  "region": "eu-west-1",
  "identifier": "cache-prod",
  "instance_type": "t4g.small",
  "storage": { "type": "gp3", "size": 20, "iops": 3000, "throughput": 125 },
  "username": "cacheuser",
  "custom_password": "CorrectHorse9",
  "db_version": "7.2",
  "public_access": true,
  "allowed_cidr_ranges": [{ "name": "office", "cidr": "203.0.113.0/24", "port": 6379 }],
  "backup_enabled": true
}
JSON
# → data: { organization_id, group_id, pid: "awsinst_…", username, host, credentials_path } — no password

# 3. read-only: poll until ready. `data` is an ARRAY even for one instance.
selfhost api /aws/v1/instances/awsinst_0123456789abcdef0123 --org acme -o json
# → data[0].status: pending → provisioning → provisioned → running
#   data[0].ready, data[0].readiness_detail, data[0].task_progress
#   failure: data[0].status == "failed" with data[0].error_message

# 4. read-only: credentials — this is the only read that returns them
#    data[0].username (default "admin"), data[0].init_pg_pass, data[0].db_port (6379),
#    data[0].dns_name / data[0].public_ip

# 5. destructive: tear down (array -> raw JSON body); create_snapshot keeps an EBS snapshot
echo '{"pids":["awsinst_0123456789abcdef0123"],"create_snapshot":true}' \
  | selfhost api /aws/v1/instances/delete --org acme --input -
```

Project database (scoped by project pid, not org):

```sh
selfhost api /api/v1/platform/projects/prj_0123456789abcdef/databases/redis --input - <<'JSON'
{ "name": "cache", "redis_password": "CorrectHorse9", "is_public": true, "public_port": 6380 }
JSON
# → 202; data.database.status is "creating" (host ready) or "pending" (host still provisioning)

# poll: data.database.status (creating → active; failed carries data.database.error_message)
selfhost api /api/v1/platform/projects/prj_0123456789abcdef/databases/prj_rd_0123456789abcdef
```

## Day-2 operations

- **Start / stop / reboot** — `PUT /aws/v1/instances/{stop,start,reboot}` with a `pids` array. Send a
  **master** pid; only stop/start expand to the instance's whole group, reboot acts on the masters
  given (`instances_controller.rb:1846,1925,2016-2034`). Synchronous, no job. Wrong state → 422.
- **Resize** (`redis resize`) — `PUT /aws/v1/instances/:id/scale`; `target_instance_type` or
  `new_size_gb`/`new_iops`/`new_throughput`. Redis is `:provision_switchover` with zero-data-loss
  promotion, so instance-type resize is allowed. Async; returns `data.scaling_event_id`.
- **Scale replicas** (`redis scale`) — `PATCH /aws/v1/instances/:id` with `multi_az` and
  `replica_count`; queues asynchronous replica add/remove. Requires `running`; enabling is billable.
- **Failover** — `POST /aws/v1/instances/:id/trigger_failover`; requires a paid plan, a master,
  `multi_az`, and at least one running replica. Async (`FailoverJob`).
- **Fork** — `POST /aws/v1/instances/:id/fork` with `instance_name`; billable, async.
- **Users** — create/update/rotate/delete per the table. `rotate_password` returns the new secret
  once under `data.meta.new_password`.
- **Backups** — create an `aws_backup_job` (EBS/DLM); `backup_enabled` / DLM policies give scheduled
  backups. Restore is **not** this endpoint — see Gotchas.
- **Snapshots** — `POST /aws/v1/volumes/create_snapshot` (`volume_type=data`); list/delete via
  `/aws/v1/snapshots`.

## Gotchas

- **PITR: not supported.** `redis_adapter.rb` leaves `supports_pitr?` false and ships no
  `pitr_*` scripts (`redis_adapter.rb:118-120`); `POST /aws/v1/instances/:instance_id/pitr` → 422.
- **AWS logs and db_stats: not supported.** The adapter overrides neither flag, so both inherit the
  base `false` (`base.rb:252-254,261-263`) → 422. (On the *project* family, logs/stats are plain
  Docker container reads and do work.)
- **Dashboards: OpenSearch-only.** `GET …/dashboards` → 422 unless
  `type_of_dbms == "opensearch"` (`instances_controller.rb:3387-3390`).
- **Backup restore for Redis is a new instance, not a restore call.** `POST /backups/:id/restore`
  admits only `clickhouse_native`/`kafka_native` → 422 (`backups_controller.rb:306-310`). Restore a
  Redis EBS snapshot by creating an instance with `snapshot_id`; the engine is enforced from the
  snapshot's tags (`instances_controller.rb:577-605`).
- **Type `redis`, not `valkey`.** `valkey` appears in the controller's tier lists but is not in the
  adapter registry, so it fails `type_of_dbms` validation (`registry.rb:3-13`). Versions: `7.0`,
  `7.2`; default `7.0` (`redis_adapter.rb:32-34`, `base.rb:45-47`).
- **`show` returns an array.** `GET /aws/v1/instances/:pid` renders `data` as a one-element array
  (`instances_controller.rb:1276-1304`); read `data[0]`.
- **`connection_limit` is rejected for Redis.** It is accepted by the API permit list but the model
  only allows it for postgres/mysql → 422 (`database_user.rb:46,181`). Usernames are checked against
  a platform reserved list that includes `admin` and `default` (`database_user.rb:27-41`).
- **Create returns no password.** It hands back `pid`/`host`/`credentials_path` only; the credential
  is readable on `GET /aws/v1/instances/:pid` (`init_pg_pass`, `instances_controller.rb:1154-1167`).
- **`database_name` is null and `extensions` is `[]` for Redis** in the instance serializer
  (`cloud_instance_serializer.rb:119,133,303-311`).
- **Project database list is not filtered by engine.** `GET …/databases` returns postgres, mysql,
  mongodb and redis rows together; filter by `db_type` client-side.
- **Deleting a project database needs the exact `name`**; a mismatch is 422
  (`databases_controller.rb:171-175`). The row is soft-deleted synchronously and disappears from the
  list immediately while the cleanup job runs.
- **`organization_id` is ignored on project routes** — that controller skips org resolution and
  derives the org from the project pid (`databases_controller.rb:9-10,220`). Only the project pid
  scopes them.
- **Billable / paid-plan triggers:** instance create, fork, replica enable, instance-type resize, and
  backup/snapshot creation. User management, logs, stats and failover require a paid plan
  (`database_users_controller.rb:9`, `logs_controller.rb:12`, `db_stats_controller.rb:16`,
  `instances_controller.rb:3424-3427`) and answer 422 "Top up your balance" otherwise.

## Sources

- Routes: `config/routes.rb:33-46` (project databases + project metrics), `:320-324` (old `/v1` →
  `/aws/v1` redirects), `:326-360` (instances + users + pitr), `:362-366` (logs, db_stats),
  `:413-427` (backups, volumes, snapshots, backup_policies), `:435-445` (regions, storage types,
  postgres extensions, pricing).
- Controllers: `app/controllers/aws/v1/instances_controller.rb`, `database_users_controller.rb`,
  `backups_controller.rb`, `snapshots_controller.rb`, `volumes_controller.rb`,
  `backup_policies_controller.rb`, `logs_controller.rb`, `db_stats_controller.rb`,
  `pricing_controller.rb`,
  `app/controllers/api/v1/platform/databases_controller.rb`,
  `app/controllers/api/v1/platform/coolify_projects_controller.rb`,
  `app/controllers/concerns/container_inspectable.rb`,
  `app/controllers/concerns/ebs_backup_capability_guard.rb`.
- Adapters/services: `app/services/database_adapters/{base,redis_adapter,registry}.rb`,
  `app/services/database_provisioning/redis_strategy.rb`,
  `app/services/redis/database_validator.rb`, `app/services/database_status_mapper.rb`.
- Serializers/models: `app/serializers/cloud_instance_serializer.rb`,
  `app/serializers/database_user_serializer.rb`, `app/models/cloud_instance.rb`,
  `app/models/database_user.rb`, `app/models/coolify_database.rb`,
  `app/models/coolify_container_fetch.rb`.
