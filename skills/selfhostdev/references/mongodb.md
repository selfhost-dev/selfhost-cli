# MongoDB

Two unrelated families live behind the `mongo` group, and they do not share a pid space:

- **Cloud instances** (`/aws/v1/...`): a `CloudInstance` row with `type_of_dbms: "mongo"`, its own
  EC2 host, EBS data volume and agent. pid `awsinst_…`, group `grp_…`. Org-scoped.
- **Managed project databases** (`/api/v1/platform/projects/:project_id/databases/mongodb`): a
  MongoDB container inside one project's box, provisioned through Coolify. pid `prj_mo_…`,
  `db_type: "mongodb"`. Scoped to the project, not to an org.

Every typed command (`selfhost mongo list|show|create|delete|start|stop|reboot|fork|resize|scale|failover|update|wait|logs|stats|metrics`, plus `mongo snapshots …` and `mongo backups …`) answers
`not implemented yet: mongo <verb>` (exit 1). Use `selfhost api` with the paths below.

MongoDB runs no script for user management, config tuning, PITR, pooling or promotion, so most of the
day-2 surface is unavailable here (see [Gotchas](#gotchas)).

## Before you start

```sh
selfhost auth status                      # signed-in profile
selfhost org use acme                     # every /aws/v1/* call needs a resolved org
```

Then read [api.md](api.md). The rules you will lean on here:

- A GET that carries parameters needs `-X GET`; a bare `-f`/`-F` turns the call into a POST.
- `{org}` in a path is replaced by the resolved org pid; the CLI also injects `organization_id`
  itself — on the query for GET/`--input` calls, into the JSON body for `-f`/`-F` writes. The
  `/aws/v1/*` controllers honor `organization_id` and its alias `organization_pid`
  (`app/controllers/application_controller.rb:123-124`); an org the request names resolves exactly
  or not at all (a pid of another org is cleared by the membership check → 404, never a silent
  fallback, `:140-142`, `:168`). Only a request that names no org falls back to your oldest
  membership (`:134-135`).
- `-f` values are strings, `-F` types `true`/`false`/integers. Nested or array bodies (`storage`,
  `pids`, `backup`) cannot be expressed with `-f` — use `--input -` with raw JSON.
- Exit codes: 2 usage, 3 not signed in, 4 billing (402), 75 rate-limited. Secrets come back in
  `data`; never log them.

## Endpoint map

### Cloud instances, lifecycle (`/aws/v1`)

| Future CLI command | Method and path | What it does | Key parameters | Source |
|---|---|---|---|---|
| `mongo list` | `GET /aws/v1/instances/by_organization` | every instance of the org, grouped by `group_id` (master + replicas) | — | `config/routes.rb:330`, `app/controllers/aws/v1/instances_controller.rb:1307` |
| `mongo show`, `mongo metrics` | `GET /aws/v1/instances/:pid` | one instance + latest agent `metrics`; alias of `/details?pid=` | — | `config/routes.rb:346`, `instances_controller.rb:1221`, `:1225` |
| — (refresh) | `POST /aws/v1/instances/refresh_by_group_id` | re-read state from AWS for a group | `group_id` | `config/routes.rb:332`, `instances_controller.rb:3363` |
| `mongo create` | `POST /aws/v1/instances` | creates the row, starts async provisioning (**billable**) | `identifier`, `type_of_dbms`, `region`, `instance_type`, `storage{}` required | `config/routes.rb:328`, `instances_controller.rb:108` |
| `mongo delete` | `POST /aws/v1/instances/delete` | deletes the pids (**destructive**; refuses a master without its replicas) | `pids[]`, `create_snapshot` | `config/routes.rb:331`, `instances_controller.rb:1346` |
| `mongo stop` | `PUT /aws/v1/instances/stop` | stops the group (master + its replicas) | `pids[]` | `config/routes.rb:333`, `instances_controller.rb:1822` |
| `mongo start` | `PUT /aws/v1/instances/start` | starts the group | `pids[]` | `config/routes.rb:334`, `instances_controller.rb:1901` |
| `mongo reboot` | `PUT /aws/v1/instances/reboot` | EC2 reboot, masters only — replicas in `pids[]` are silently dropped, and a batch with no master is refused | `pids[]` | `config/routes.rb:335`, `instances_controller.rb:1990` |
| `mongo update` | `PATCH /aws/v1/instances/:pid` | mutable attributes (tags, access, names, flags) | `identifier`, `tags{}`, `public_access`, `allowed_cidr_ranges[]`, `multi_az`, `replica_count`, `backup_enabled`, `delete_protection`, `db_port` | `config/routes.rb:336`, `instances_controller.rb:2045`, `:2070` |
| `mongo fork` | `POST /aws/v1/instances/:pid/fork` | clones from an EBS snapshot into a new instance (**billable**) | `instance_name` (required), `cloud_credential_id` | `config/routes.rb:337`, `instances_controller.rb:2845` |
| `mongo resize` | `PUT /aws/v1/instances/:pid/scale` | instance-type and/or storage resize | `target_instance_type`, `new_size_gb`, `new_iops`, `new_throughput`, `new_volume_type` | `config/routes.rb:338`, `instances_controller.rb:3151` |
| `mongo failover` | `POST /aws/v1/instances/:pid/trigger_failover` | promotes a replica (see gotchas — broken for mongo); paid-plan gated | `rpo_override` — passing true without a quorum-deny record for the instance in the last 30 minutes is a 422 | `config/routes.rb:339`, `instances_controller.rb:3411`, `:3424`, `:3499` |
| — (dashboards) | `GET /aws/v1/instances/:pid/dashboards` | OpenSearch only; 422 for mongo | — | `config/routes.rb:340`, `instances_controller.rb:3388` |

### Pricing (`/aws/v1/pricing`)

Two reads that answer without a session and without an organization — the controller skips both
auth filters (`app/controllers/aws/v1/pricing_controller.rb:6-8`) and both paths sit in the
Firebase auth skip list (`config/application.rb:40`) — under a shared limit of 60 requests a minute
(`pricing_controller.rb:10-11`). Neither bills nor mutates. Field-by-field coverage lives in
[catalog.md](catalog.md).

| Future CLI command | Method and path | What it does | Key parameters | Source |
|---|---|---|---|---|
| — (catalog) | `GET /aws/v1/pricing` | instance-type price list for one region, plus what the monthly price includes and does not | `region` (default `us-east-1`) | `config/routes.rb:444`, `app/controllers/aws/v1/pricing_controller.rb:13-20` |
| — (quote) | `POST /aws/v1/pricing/estimate` | monthly quote for one shape; creates nothing, so it is safe to call before create | `region`, `instance_type`, `mode` (`managed`/`byoc`), `db_type` (`mongo`), `storage_type`, `storage_size_gb`, `database_count`, `replica_count`, `public_ipv4_count`, `iops`, `throughput_mbps` | `config/routes.rb:445`, `pricing_controller.rb:22-31`, `:35-49` |

Quote the shape you are about to create — a replica set is `database_count: 1` plus
`replica_count: n`, and both are priced as full units
(`app/services/pricing/hybrid_pricing_service.rb:64`). `db_type` only selects the engine margin
override, and mongo has none, so the quote carries the SKU default (`db/seeds/skus.rb:337-341`).
An unknown region, mode, instance type or storage type, or a size/IOPS/throughput outside what the
storage type offers, comes back 422 with the service's own message (`pricing_controller.rb:29-30`,
`hybrid_pricing_service.rb:208-254`). So does a numeric field that is not a number or sits below
its minimum (`integer_param` / `decimal_param`, `hybrid_pricing_service.rb:387-403`), and a type
with no price seeded for that region: the quote prices from the same SKU rows the bill uses, so an
unpriced shape fails closed with `Pricing not available for <type> in <region>` (`:312-317`).

### Users, logs, stats, PITR — not available for MongoDB

| Future CLI command | Method and path | What you actually get | Source |
|---|---|---|---|
| `mongo … users` (no verb registered) | `GET/POST /aws/v1/instances/:pid/database_users`, `PATCH/DELETE …/:id`, `POST …/:id/rotate_password` | the row is created, then the sync job raises: mongo has no `user_management` script | `config/routes.rb:348-350`, `app/services/cloud_provider/aws/task_creation_service.rb:506`, `app/services/database_adapters/mongo_adapter.rb:74` |
| `mongo logs` | `POST /aws/v1/instances/:pid/logs`, `GET` to poll | only the dispatch call answers 422 for mongo (`Log fetching is not supported for mongo instances`); the poll has no engine gate and returns 200 with the recent-fetch list (empty for mongo) | `config/routes.rb:362-364`, `app/controllers/aws/v1/logs_controller.rb:28`, `:44` |
| `mongo stats` | `POST /aws/v1/instances/:pid/db_stats`, `GET` to poll | only the dispatch call answers 422 for mongo (`Query stats are not supported for mongo instances`); the poll has no engine gate and returns 200 with the recent-fetch list (empty for mongo) | `config/routes.rb:366`, `app/controllers/aws/v1/db_stats_controller.rb:19`, `:32` |
| — (no PITR verb) | `GET/POST/DELETE /aws/v1/instances/:pid/pitr` | only the enable call answers 422 for mongo (`PITR is not supported for mongo instances`); the status read returns 200 with an empty PITR slot when nothing is configured, and the disable call answers 404 when nothing is configured | `config/routes.rb:352`, `app/controllers/aws/v1/pitr_controller.rb:23`, `:38`, `:270` |

### Backups (`/aws/v1`) — EBS/DLM only

| Future CLI command | Method and path | What it does | Key parameters | Source |
|---|---|---|---|---|
| `mongo backups list` | `GET /aws/v1/backups` | syncs then lists `Backup` rows for the org | `organization_id` (injected) | `config/routes.rb:413`, `app/controllers/aws/v1/backups_controller.rb:22` |
| `mongo backups create` | `POST /aws/v1/backups` | starts an AWS Backup job (**billable**) | `backup{backup_type:"aws_backup_job", resource_id:<instance pid>, organization_id, region, options{backup_vault_name, iam_role_arn}}` | `config/routes.rb:413`, `backups_controller.rb:50`, `:15` |
| `mongo backups delete` | `DELETE /aws/v1/backups/:id` | removes the record (**destructive**) | — | `config/routes.rb:413`, `backups_controller.rb:538` |
| `mongo backups restore` | `POST /aws/v1/backups/:id/restore` | 422 — native restore is ClickHouse/Kafka only; restore a mongo snapshot by creating an instance | `target_instance_id` | `config/routes.rb:416`, `backups_controller.rb:308` |

### Snapshots and backup policies (`/aws/v1`)

| Future CLI command | Method and path | What it does | Key parameters | Source |
|---|---|---|---|---|
| `mongo snapshots list` | `GET /aws/v1/snapshots` | **account-wide** EBS snapshots in one region — not instance- or org-scoped | `region` (default `us-east-1`), `cloud_credential_id` | `config/routes.rb:425`, `app/controllers/aws/v1/snapshots_controller.rb:12`, `app/services/cloud_provider/aws/aws_backup_service.rb:439` |
| `mongo snapshots show` | `GET /aws/v1/snapshots/:id` | one AWS snapshot | `region` | `config/routes.rb:425`, `snapshots_controller.rb:49` |
| `mongo snapshots create` | `POST /aws/v1/volumes/create_snapshot` | EBS snapshot of the instance's volume (**billable**) | `pid`, `organization_id` (injected by the CLI; the controller requires it, and that org is the one the snapshot SKU is billed to), `volume_type` (`data`/`root`) — `root` resolves the root volume and returns a silent no-op success (a completed record with no snapshot id), so use `data` for a restorable snapshot | `config/routes.rb:419-421`, `app/controllers/aws/v1/volumes_controller.rb:13`, `:30`, `:32-33`, `app/services/cloud_provider/aws/aws_backup_service.rb:35` |
| `mongo snapshots restore` | `POST /aws/v1/instances` with `snapshot_id` | creates a new instance from the snapshot (**billable**) | `snapshot_id`, `region` | `config/routes.rb:328`, `instances_controller.rb:427` |
| `mongo snapshots delete` | `DELETE /aws/v1/snapshots/:id` | deletes the AWS snapshot (**destructive**) — 404s without a local record matching the snapshot id, and the region comes from that record, not request parameters | — | `config/routes.rb:425`, `snapshots_controller.rb:103`, `:108`, `:114` |
| — (policies) | `POST/GET /aws/v1/backup_policies`, `PATCH/DELETE /aws/v1/backup_policies/:id` | DLM policies that snapshot volumes by tag | create takes flat `policy_name`, `description`, `schedule_interval`, `schedule_unit`, `retain_rule_count`, `target_tags[]` (+ `region`, `cloud_instance_pid`); update takes `description`, `execution_role_arn`, `state` plus nested `policy_details` and ignores the top-level schedule fields | `config/routes.rb:427`, `app/controllers/aws/v1/backup_policies_controller.rb:19`, `:44`, `:145`, `:253`, `:947` |

### Managed project database (Coolify path)

`:project_id` is the project pid (`prj_…`); `:db_type` is the literal `mongodb`.

| Future CLI command | Method and path | What it does | Key parameters | Source |
|---|---|---|---|---|
| `mongo list` (project) | `GET /api/v1/platform/projects/:project_id/databases` | databases of one project | — | `config/routes.rb:37`, `app/controllers/api/v1/platform/databases_controller.rb:30` |
| `mongo show` | `GET /api/v1/platform/projects/:project_id/databases/:pid` | one database; refreshes status from Coolify and reveals `connection.password` | — | `config/routes.rb:38`, `databases_controller.rb:45` |
| `mongo create` | `POST /api/v1/platform/projects/:project_id/databases/:db_type` | declares + provisions the container, 202 (no billing gate on this path — the database rides its project's box) | `name`, `mongo_initdb_root_username`, `mongo_initdb_root_password`, `mongo_initdb_database`, `is_public`, `public_port` (`instant_deploy` is accepted but has no effect — it is stripped before the configuration is saved and never sent on) | `config/routes.rb:45`, `databases_controller.rb:87`, `:140`, `app/services/mongodb_database_validator.rb:12` |
| `mongo logs` | `POST …/databases/:pid/logs`, then `GET` to poll | dispatch a `docker logs` fetch, 202 | `lines` (default 200, max 1000) | `config/routes.rb:41-42`, `app/controllers/concerns/container_inspectable.rb:24` |
| `mongo stats` | `POST …/databases/:pid/stats`, then `GET` to poll | dispatch a `docker stats` fetch, 202 | `lines` is accepted but ignored for stats — the limit applies to logs only | `config/routes.rb:43-44`, `container_inspectable.rb:25`, `app/services/coolify/inspection/container_inspector.rb:72` |
| `mongo delete` | `DELETE /api/v1/platform/projects/:project_id/databases/:pid` | soft-delete now, Coolify cleanup async (**destructive**) | `name` must equal the database's name | `config/routes.rb:46`, `databases_controller.rb:167` |

## Provision one end to end (cloud instance)

```sh
# 0. read-only, public (no auth): region, instance types, storage types
selfhost api -X GET /aws/v1/regions
selfhost api -X GET /aws/v1/regions/us-east-1/instance_types -f family=m7g
selfhost api -X GET /aws/v1/storage_types

# 0b. quote the shape first — read-only, public, creates nothing
selfhost api /aws/v1/pricing/estimate \
  -f region=us-east-1 -f instance_type=m7g.large -f db_type=mongo \
  -f storage_type=gp3 -F storage_size_gb=100 -F database_count=1 -F replica_count=1 -o json

# 1. create — billable. Nested storage{} needs a raw body.
selfhost api /aws/v1/instances --input - <<'JSON'
{
  "identifier": "orders-mongo",
  "type_of_dbms": "mongo",
  "region": "us-east-1",
  "instance_type": "m7g.large",
  "db_version": "7.0",
  "storage": { "type": "gp3", "size": 100, "iops": 3000, "throughput": 125 },
  "username": "admin",
  "public_access": true,
  "allowed_cidr_ranges": [{ "cidr": "203.0.113.7/32", "port": 27017 }],
  "backup_enabled": true,
  "delete_protection": false
}
JSON
```

Omit `custom_password` to have one generated (it becomes `init_pg_pass`); if you pass it, it must
satisfy the platform password policy (`instances_controller.rb:3902`). `db_version` defaults to
`6.0` and is validated against `6.0`, `7.0`, `8.0` (`app/services/database_adapters/mongo_adapter.rb:15`,
`app/models/cloud_instance.rb:2100`). Required: `region`, `identifier`, `instance_type`, `storage`,
`type_of_dbms` (`app/validators/aws_validators/instances_validator.rb:24`, `:46`, `:86`).

Response (HTTP 200; the password is deliberately absent): `data.pid`, `data.group_id`, `data.username`,
`data.host`, `data.credentials_path` (`instances_controller.rb:1152`).

```sh
# 2. wait — poll this until data[0].ready is true (every 15-30 s is plenty).
#    data is an ARRAY; the row you want is the master.
selfhost api -X GET /aws/v1/instances/awsinst_xxxxxxxxxx -o json
```

`ready` is one boolean over status/internal_status/error/agent health (`cloud_instance.rb:1853`).
Read `readiness_detail` for the human reason (`reasons`, `stuck`) and `data[0].error_message` /
`data[0].provisioning_state` when it stalls. Failed provisioning ends `status: "failed"` with
`error_message` set.

```sh
# 3. credentials (read again, the create response deliberately omits the password)
selfhost api -X GET /aws/v1/instances/awsinst_xxxxxxxxxx -o json
#    data[0].username === "admin", data[0].init_pg_pass === root password,
#    data[0].effective_port === 27017, data[0].dns_name / public_ip
```

The installer creates the root user in the `admin` database with
`root/userAdminAnyDatabase/readWriteAnyDatabase/dbAdminAnyDatabase/clusterAdmin` and always
initiates a single-member replica set `rs0`, even on a single node
(`script/mongo_install.sh:243`, `:264`). So the URI is
`mongodb://<username>:<init_pg_pass>@<public_ip or dns_name>:27017/admin?replicaSet=rs0` —
`admin` is the auth database and `rs0` is required (`app/services/database_adapters/mongo_adapter.rb:32`).

```sh
# 4. tear down — destructive, and it refuses a master without its replicas.
selfhost api /aws/v1/instances/delete --input - <<'JSON'
{ "pids": ["awsinst_xxxxxxxxxx"], "create_snapshot": true }
JSON
```

`create_snapshot: true` takes an EBS snapshot of every non-root volume first and keeps it as a
`Backup` row (`pre_delete_backup_strategy` is `ebs_snapshot` for every volume-backed engine,
`app/services/database_adapters/base.rb:124`, `instances_controller.rb:1507`, `:1518`); that row's
`aws_snapshot_id` is what `mongo snapshots restore` feeds back into create.

## Day-2 operations

| Operation | Call | Works for mongo? |
|---|---|---|
| stop / start / reboot | `PUT /aws/v1/instances/{stop,start,reboot}` with `{"pids":[…]}` | yes — stop/start act on the whole group, reboot is masters only (replicas in the batch are dropped) |
| resize instance type or disk | `PUT /aws/v1/instances/:pid/scale` | yes, instance type included: `resize_strategy` is `:in_place` (`app/services/database_adapters/mongo_adapter.rb:42`) |
| more replicas | `PATCH /aws/v1/instances/:pid` with `multi_az: true`, `replica_count: n` | enable HA only; a `multi_az` PATCH equal to the current value is a no-op, so replica count cannot be raised on a live HA group (`instances_controller.rb:2399`, `:2447`) |
| rename, tags, CIDR, public access | `PATCH /aws/v1/instances/:pid` | yes |
| `db_version` change | `PATCH` | no — `db_version` is ClickHouse-only day-2 (`instances_controller.rb:2257`) |
| fork | `POST /aws/v1/instances/:pid/fork` with `instance_name` | yes, snapshot-based |
| backup / restore | AWS Backup job, DLM policy, or EBS snapshot | yes, EBS-backed (`database_adapters/base.rb:107`) |
| manual EBS snapshot | `POST /aws/v1/volumes/create_snapshot` | yes |
| restore a snapshot | `POST /aws/v1/instances` with `snapshot_id` | yes; the engine is forced from the snapshot's origin/tags (`instances_controller.rb:591`) |
| users | `/database_users*` | no |
| logs / query stats / PITR / failover | see map | no |

## Gotchas

- **No user management.** `MongoAdapter#scripts` has no `:user_management` entry, so
  `create_database_user_task` raises `Database user management not supported for mongo`
  (`task_creation_service.rb:506`). `POST …/database_users` still returns 201 with `status:
  "pending"`, then `DatabaseUserSyncJob` raises before a task exists, retries 3× and dies
  (`app/jobs/database_users/database_user_sync_job.rb:9`, `:69`) — no user is ever created on the
  box and the row never leaves `pending`. Do not offer per-user credentials.
- **Failover is a stub.** `script/mongo_promote.sh` prints `MongoDB manual promotion not implemented`
  and `exit 1`, and every `MongoPromotionService` method raises `NotImplementedError`
  (`app/services/mongo_promotion_service.rb`). `trigger_failover` passes its preconditions (multi-AZ,
  master, running replica) and dispatches that task, so a failover looks accepted but never
  promotes. `supports_zero_data_loss_promotion?` is false and the model notes mongo has "no
  multi-peer HA promotion today" (`mongo_adapter.rb:36`, `app/models/cloud_instance.rb:1175`).
- **No PITR, no log tail, no query stats.** The enable/dispatch calls 422 for mongo (`PITR is not supported`, `Log fetching is not supported`, `Query stats are not supported`); the status read and the log/stats polls have no engine gate and return 200 with empty data when nothing was ever dispatched (`app/services/database_adapters/base.rb:74`, `:252`, `:261`).
  Billing/plan gates also apply to logs and stats endpoints (`require_paid_plan!`).
- **`GET /aws/v1/instances/:pid` returns an array** (`data` is a list of one; pass `?instance_id=` for an EC2-id lookup, which can return more rows),
  and `/details?pid=` is the same handler — poll `data[0]`, not `data` (`instances_controller.rb:1226`, `:1254`, `:1304`).
- **The root credentials are `username` + `init_pg_pass`**, DB `admin`, on `effective_port` 27017.
  `init_pg_pass` is in the serializer's safe list, so it comes back on every read of the instance;
  the create and fork responses deliberately omit it (`instances_controller.rb:1159`,
  `app/serializers/cloud_instance_serializer.rb:45`).
- **Snapshots are engine-specific.** A snapshot restores only as its originating engine, forced from
  the source record or the engine tags; asking for a different `type_of_dbms` is a 422
  (`instances_controller.rb:591`). Fork/backup snapshots capture a **crash-consistent** volume:
  only ClickHouse and OpenSearch get a pre-snapshot flush (`app/jobs/fork/fork_instance_job.rb:184`,
  `:185`).
- **`mongo backups restore` is not the restore path.** `POST /aws/v1/backups/:id/restore` only
  accepts `clickhouse_native` / `kafka_native` backups (`backups_controller.rb:308`). To restore a
  mongo backup, create a new instance with `snapshot_id` (or a new project database and move the
  data yourself).
- **AWS Backup jobs need a vault and a role.** `POST /aws/v1/backups` with `aws_backup_job` requires
  `options.backup_vault_name` plus a resolvable IAM role (`options.iam_role_arn` or an
  org/AWS default), and the instance must be `running` or `stopped` (`backups_controller.rb:128`,
  `:103`). A started job stays `in_progress` until `ReconcileBackupStatusJob` sees AWS complete it.
- **`GET /aws/v1/snapshots` is account-wide, not org-scoped.** `list_snapshots` logs the org pid
  and then lists every snapshot the credential can see, `describe_snapshots(owner_ids: ["self"])`
  (`app/services/cloud_provider/aws/aws_backup_service.rb:432`, `:439`). It also defaults to
  `us-east-1`, so always pass `region` and filter the result yourself
  (`snapshots_controller.rb:13`).
- **Project-database pids are `prj_mo_…`** (`app/models/coolify_database.rb:119`). `db_type` in the
  path is `mongodb`, and the create validator allows only letters/digits/underscores for
  `mongo_initdb_root_username` and `mongo_initdb_database` (`mongodb_database_validator.rb:13`),
  while the password may not contain spaces or `` ` $ ; | & < > \ ' " `` (`:51`). The root password
  is returned **only** by the project-database `show` call (`connection.password`);
  `index`/`create` omit it (`databases_controller.rb:75`, `coolify_database.rb:62`).
- **Project-database create can 409** on a port clash with another child of the same project or on
  host capacity (`databases_controller.rb:121`, `:128`), and `destroy` requires body/query `name`
  equal to the database's display name (`:167`). Logs/stats need the project's agent to be
  available, otherwise the dispatch 422s.

## Sources

- `config/routes.rb:328-346` (aws/v1 instances), `:348-350` (database users), `:352-361` (pitr),
  `:362-364` (logs), `:366` (db_stats), `:413-417` (backups), `:419-423` (volumes), `:425`
  (snapshots), `:427` (backup policies), `:435-438` (regions, storage types), `:444-445` (pricing),
  `:37-46` (platform project databases).
- Controllers: `app/controllers/aws/v1/instances_controller.rb`,
  `database_users_controller.rb`, `backups_controller.rb`, `snapshots_controller.rb`,
  `volumes_controller.rb`, `backup_policies_controller.rb`, `logs_controller.rb`,
  `db_stats_controller.rb`, `pitr_controller.rb`, `pricing_controller.rb`,
  `app/controllers/api/v1/platform/databases_controller.rb`, `app/controllers/concerns/container_inspectable.rb`.
- Adapters and strategies: `app/services/database_adapters/base.rb`, `mongo_adapter.rb`,
  `app/services/database_provisioning/mongodb_strategy.rb`, `app/services/mongodb_database_validator.rb`,
  `app/services/mongo_promotion_service.rb`, `app/services/cloud_provider/aws/task_creation_service.rb`,
  `app/jobs/fork/fork_instance_job.rb`, `app/jobs/provision_database_job.rb`,
  `app/jobs/failover_job.rb`, `script/mongo_install.sh`,
  `mongo_replication.sh`, `mongo_promote.sh`.
- Models and serializers: `app/models/cloud_instance.rb`, `app/models/coolify_database.rb`,
  `app/serializers/cloud_instance_serializer.rb`, `backup_serializer.rb`.
