# ClickHouse

Managed ClickHouse (`type_of_dbms=clickhouse`, 26.8 LTS only) on cloud instances. A database is a
`CloudInstance` whose pid looks like `awsinst_…`, exposed through the shared instance API under
`/aws/v1/instances`; replicas and user rows carry their own pids, and everything is
organization-scoped through `organization_id` (a generated `org_…` pid, never a slug).
`selfhost clickhouse …` answers `not implemented yet: clickhouse <verb>` today, so use `selfhost api`.

## Before you start

```sh
selfhost auth status              # exit 3 means: selfhost auth login
selfhost org use acme             # or pass --org acme / SELFHOSTDEV_ORG on each call
```

Read [api.md](api.md) first: leading-slash path, `-X GET` when a GET carries parameters (a bare `-f`
turns the call into a POST), `-f` = raw string vs `-F` = typed (`true`, `null`, `42`), `--input -` =
raw JSON body from stdin, exit codes (1 error, 2 usage, 3 auth, 4 billing, 75 rate limit), `{org}` =
org **pid**. Three ClickHouse-specific points:

- The CLI appends the resolved organization's `organization_id` to the query of a GET or `--input`
  request, and into the JSON body of a write made with `-f`/`-F`.
- `create`, `clickhouse users create|update` and `backups create` read **nested** keys
  (`storage`, `database_user`, `backup`); `-f` sends flat keys only, so send those bodies as raw JSON.
- Backup policy create/update take flat top-level params (`organization_id`, `cloud_instance_pid`,
  `schedule_interval`, `schedule_unit`, `retain_rule_count`, `drill_interval`/`drill_unit`, `state`),
  so plain `-f key=value` flags work there.
- `init_pg_pass` (the DB password) is returned on the authenticated instance **read**, not on create.

## Endpoint map

**Instance lifecycle**

| Future CLI command | Method and path | What it does | Key parameters | Source |
|---|---|---|---|---|
| `clickhouse list` | `GET /aws/v1/instances/by_organization` | Live instances in your org, one array entry per group: each carries `group_id`, `cloud_provider`, `region` and a nested `instances.master` plus `instances.replicas` (master is null in a replica-only group) | – | `config/routes.rb:330`, `app/controllers/aws/v1/instances_controller.rb:1307,1321-1343` |
| `clickhouse show` | `GET /aws/v1/instances/{pid}` | Canonical read: readiness, credentials, metrics (`/instances/details?pid=` is the same payload) | `pid` (path) | `config/routes.rb:346,329`, `instances_controller.rb:1221` |
| – | `POST /aws/v1/instances/refresh_by_group_id` | Re-read EC2 state for a group | `group_id` | `config/routes.rb:332`, `instances_controller.rb:3363` |
| `clickhouse create` | `POST /aws/v1/instances` | Provision (billable) | `type_of_dbms`,`region`,`identifier`,`instance_type`,`storage{…}` | `config/routes.rb:328`, `instances_controller.rb:108` |
| `clickhouse delete` | `POST /aws/v1/instances/delete` | Terminate (destructive) | `pids: []`, `create_snapshot` | `config/routes.rb:331`, `instances_controller.rb:1346` |
| `clickhouse stop` | `PUT /aws/v1/instances/stop` | Stop a group | `pids: []` | `config/routes.rb:333`, `instances_controller.rb:1822` |
| `clickhouse start` | `PUT /aws/v1/instances/start` | Start a stopped group (billing-gated) | `pids: []` | `config/routes.rb:334`, `instances_controller.rb:1901` |
| `clickhouse reboot` | `PUT /aws/v1/instances/reboot` | Reboot masters that are `running` | `pids: []` | `config/routes.rb:335`, `instances_controller.rb:1990` |
| `clickhouse update` | `PATCH /aws/v1/instances/{id}` | Tags, delete protection, public access, backup, HA | see Day-2 | `config/routes.rb:336`, `instances_controller.rb:2045,2070` |
| `clickhouse fork` | `POST /aws/v1/instances/{id}/fork` | Clone into a new single-node instance (billable) | `instance_name`,`cloud_credential_id` | `config/routes.rb:337`, `instances_controller.rb:2845` |
| `clickhouse resize` | `PUT /aws/v1/instances/{id}/scale` | Change instance type and/or EBS size/IOPS/throughput | `target_instance_type`,`new_size_gb`,`new_iops`,`new_throughput`,`new_volume_type` | `config/routes.rb:338`, `instances_controller.rb:3151,3156` |
| `clickhouse failover` | `POST /aws/v1/instances/{id}/trigger_failover` | Promote a replica on a multi-AZ cluster | `rpo_override` | `config/routes.rb:339`, `instances_controller.rb:3411,3493` |
| `clickhouse metrics`,`wait` | `GET /aws/v1/instances/{pid}` | `metrics` field of the read; poll `ready` for `wait` | `pid` (path) | `instances_controller.rb:1276`, `app/serializers/cloud_instance_serializer.rb:109` |

`GET /aws/v1/instances/{id}/dashboards` (`config/routes.rb:340`) is OpenSearch-only and answers 422
for ClickHouse (`instances_controller.rb:3388`). ClickHouse has no Dashboards, no `config`, no `pitr`.

**Database users** — `role` is `read_only`, `read_write` or `admin` (`app/models/database_user.rb:7`).

| Future CLI command | Method and path | What it does | Key parameters | Source |
|---|---|---|---|---|
| `clickhouse users list` | `GET /aws/v1/instances/{instance_id}/database_users` | List live users | – | `config/routes.rb:348`, `app/controllers/aws/v1/database_users_controller.rb:27` |
| `clickhouse users create` | `POST /aws/v1/instances/{instance_id}/database_users` | Create a user on the engine (paid plan); a connection limit is refused for ClickHouse | `database_user{username,password,role,connection_limit}` | `config/routes.rb:348`, `database_users_controller.rb:35` |
| `clickhouse users update` | `PATCH /aws/v1/instances/{instance_id}/database_users/{id}` | Change role/limits/expiry; on ClickHouse only role, expiry and the rotation flag apply | `database_user{role,connection_limit,expires_at,password_rotation_enabled}` | `config/routes.rb:348`, `database_users_controller.rb:89,109` |
| `clickhouse users rotate-password` | `POST /aws/v1/instances/{instance_id}/database_users/{id}/rotate_password` | Replace the password, revealed once | – | `config/routes.rb:349`, `database_users_controller.rb:132,159` |
| `clickhouse users delete` | `DELETE /aws/v1/instances/{instance_id}/database_users/{id}` | Delete a user (bootstrap admin refused) | – | `config/routes.rb:348`, `database_users_controller.rb:167` |

`connection_limit` is accepted as a key on both calls but ClickHouse is refused: only Postgres and
MySQL enforce a per-user limit, so a number sent to any other engine would be stored and never
applied, and the save answers 422 "Connection limit is not supported for clickhouse instances"
(`app/models/database_user.rb:46,177-183`).

**`clickhouse pool` = CHProxy** — these verbs map to CHProxy, not a native pool; the controller rejects any
non-ClickHouse instance (`chproxy_controller.rb:149`). No member id in the path.

| Future CLI command | Method and path | What it does | Key parameters | Source |
|---|---|---|---|---|
| `clickhouse pool show` | `GET /aws/v1/instances/{instance_id}/chproxy` | Pool config, or `chproxy: null` when unset | – | `config/routes.rb:384`, `app/controllers/aws/v1/chproxy_controller.rb:17` |
| `clickhouse pool enable` | `POST /aws/v1/instances/{instance_id}/chproxy` | Enable CHProxy | `max_concurrent_queries`,`max_queue_size`,`max_queue_time`,`max_execution_time` | `config/routes.rb:384`, `chproxy_controller.rb:27` |
| `clickhouse pool update` | `PUT /aws/v1/instances/{instance_id}/chproxy` | Reconfigure CHProxy | same four keys | `config/routes.rb:384`, `chproxy_controller.rb:66` |
| `clickhouse pool disable` | `DELETE /aws/v1/instances/{instance_id}/chproxy` | Disable CHProxy | – | `config/routes.rb:384`, `chproxy_controller.rb:104` |

**ClickHouse-native backups** — `POST /aws/v1/backups` accepts `backup_type=clickhouse_native` only
for ClickHouse; the EBS-snapshot restore path stays on instance create with `snapshot_id`
(`config/routes.rb:414`).

| Future CLI command | Method and path | What it does | Key parameters | Source |
|---|---|---|---|---|
| `clickhouse backups list` | `GET /aws/v1/backups` | Backups for your org | `organization_id` | `config/routes.rb:413`, `app/controllers/aws/v1/backups_controller.rb:22` |
| `clickhouse backups create` | `POST /aws/v1/backups` | Dispatch `BACKUP … TO S3` (billable: 402 without balance) | `backup{backup_type,resource_id,organization_id,region,options}` | `config/routes.rb:413`, `backups_controller.rb:50,168` |
| `clickhouse backups restore` | `POST /aws/v1/backups/{id}/restore` | Additive RESTORE into a running instance | `target_instance_id`,`databases`,`restore_cleanup_confirmed` | `config/routes.rb:416`, `backups_controller.rb:289` |
| `clickhouse backups delete` | `DELETE /aws/v1/backups/{id}` | Drop the record and its S3 objects | – | `config/routes.rb:413`, `backups_controller.rb:538` |

**EBS snapshots and backup policies** — snapshots are per-volume; the create lives on `/volumes`, not
`/snapshots` (`config/routes.rb:419`). No restore endpoint: create an instance with `snapshot_id`.

| Future CLI command | Method and path | What it does | Key parameters | Source |
|---|---|---|---|---|
| `clickhouse snapshots create` | `POST /aws/v1/volumes/create_snapshot` | EBS snapshot of one volume (billable); `volume_type` defaults to `data` | `pid`,`organization_id`,`cloud_credential_id`,`volume_type` | `config/routes.rb:421`, `app/controllers/aws/v1/volumes_controller.rb:13-18,28` |
| `clickhouse snapshots list` | `GET /aws/v1/snapshots` | EBS snapshots visible to the account | `region`,`cloud_credential_id` | `config/routes.rb:425`, `app/controllers/aws/v1/snapshots_controller.rb:20` |
| `clickhouse snapshots show` | `GET /aws/v1/snapshots/{id}` | One snapshot's state/progress | `id`,`region` | `config/routes.rb:425`, `snapshots_controller.rb:49` |
| `clickhouse snapshots delete` | `DELETE /aws/v1/snapshots/{id}` | Delete a snapshot (destructive) | `id` (path; the region comes from the stored record, a passed region is ignored) | `config/routes.rb:425`, `snapshots_controller.rb:103` |
| backup policies list | `GET /aws/v1/backup_policies` | Policies in the org, optionally one instance | `organization_id`,`cloud_instance_pid` | `config/routes.rb:427`, `app/controllers/aws/v1/backup_policies_controller.rb:19` |
| backup policies create | `POST /aws/v1/backup_policies` | Scheduled native backups + drill cadence | `organization_id`,`policy_type`,`cloud_instance_pid`,`schedule_interval`,`schedule_unit`,`retain_rule_count`,`drill_interval`,`drill_unit` | `config/routes.rb:427`, `backup_policies_controller.rb:594` |
| backup policies update | `PATCH|PUT /aws/v1/backup_policies/{id}` | Cadence, state, retention | `organization_id`,`state`,`schedule_interval`,`schedule_unit`,`retain_rule_count` | `config/routes.rb:427`, `backup_policies_controller.rb:669` |
| backup policies delete | `DELETE /aws/v1/backup_policies/{id}` | Delete policy + its scheduled backups | `organization_id` | `config/routes.rb:427`, `backup_policies_controller.rb:737` |

**Logs, query stats, reference data** — ClickHouse log sources (`app/services/database_adapters/clickhouse_adapter.rb:295`):
`engine`, `engine_error`, `native_backup`, `native_restore`, `restore`, `fork`, `replication`.

| Future CLI command | Method and path | What it does | Key parameters | Source |
|---|---|---|---|---|
| `clickhouse logs` (sources) | `GET /aws/v1/instances/{instance_id}/logs/sources` | Available log sources | – | `config/routes.rb:363`, `app/controllers/aws/v1/logs_controller.rb:17` |
| `clickhouse logs` | `GET /aws/v1/instances/{instance_id}/logs` | Last 20 fetches and their output | – | `config/routes.rb:362`, `logs_controller.rb:28` |
| `clickhouse logs` (fetch) | `POST /aws/v1/instances/{instance_id}/logs` | Dispatch a log tail (paid plan) | `log_source`,`lines` | `config/routes.rb:362`, `logs_controller.rb:42` |
| `clickhouse stats` | `GET /aws/v1/instances/{instance_id}/db_stats` | Last 20 stats fetches | – | `config/routes.rb:366`, `app/controllers/aws/v1/db_stats_controller.rb:19` |
| `clickhouse stats` (sample) | `POST /aws/v1/instances/{instance_id}/db_stats` | Dispatch a query-stats snapshot | `sample_seconds` (5–60, default 10) | `config/routes.rb:366`, `db_stats_controller.rb:30` |
| – | `GET /aws/v1/regions` | Valid regions (public) | – | `config/routes.rb:435`, `app/controllers/aws/v1/regions_controller.rb:21` |
| – | `GET /aws/v1/regions/{region_code}/instance_types` | Instance types in a region (public) | `family`,`current_gen`,`min_vcpu`,`min_memory` | `config/routes.rb:436`, `regions_controller.rb:28` |
| – | `GET /aws/v1/pricing` | Price catalog for a region: per-instance-type hourly and monthly prices, plus what the monthly price includes and bills separately (public, 60/min) | `region` (defaults to the service default) | `config/routes.rb:444`, `app/controllers/aws/v1/pricing_controller.rb:13` |
| – | `POST /aws/v1/pricing/estimate` | Cost estimate before create (public) | `region`,`instance_type`,`db_type`,`storage_type`,`storage_size_gb`,`replica_count` | `config/routes.rb:445`, `pricing_controller.rb:22` |

## Provision one end to end

1. Pick a region and instance type, then estimate the cost (all read-only, public).

   ```sh
   selfhost api /aws/v1/regions -o json
   selfhost api -X GET /aws/v1/regions/us-east-1/instance_types -f min_vcpu=4 -o json
   selfhost api /aws/v1/pricing/estimate -F region=us-east-1 -f instance_type=m6i.large \
     -f db_type=clickhouse -f storage_type=gp3 -F storage_size_gb=100 -F replica_count=1
   ```

2. Create it. **Billable.** The body is nested (`storage{}`, `tags[]`, `allowed_cidr_ranges[]`), so
   send raw JSON; `organization_id` rides the query, injected from `--org`.

   ```sh
   cat <<'JSON' | selfhost api --input - /aws/v1/instances --org acme
   {
     "type_of_dbms": "clickhouse",
     "region": "us-east-1",
     "identifier": "ch-analytics",
     "instance_type": "m6i.large",
     "storage": { "type": "gp3", "size": 100, "iops": 3000, "throughput": 125 },
     "storage_mode": "object_storage",
     "multi_az": false,
     "backup_enabled": true,
     "public_access": true,
     "allowed_cidr_ranges": [
       { "name": "office", "cidr": "203.0.113.0/24", "port": 8123, "description": "office NAT" }
     ]
   }
   JSON
   ```

   Required: `region`, `identifier`, `instance_type`, `storage` and `type_of_dbms`
   (`app/validators/aws_validators/instances_validator.rb:22-25,46,84-91`). Inside `storage`, only
   `type` is presence-checked, against gp2/gp3/io1/io2/st1/sc1/standard; `size` has a per-volume-type
   floor, `iops` is checked only for gp3 (≥ 3000) and io1/io2 (≥ 100) and silently zeroed elsewhere,
   and `throughput` is accepted but never validated (`:96-135`). `db_version` defaults to `26.8`
   (`app/services/database_adapters/clickhouse_adapter.rb:80`). Response `data`: `pid`, `group_id`, `organization_id`, `username`,
   `host`, `credentials_path` — **no password** (`instances_controller.rb:1155-1166`).

3. Poll until ready. `GET /aws/v1/instances/{pid}` returns the payload of step 4; poll `ready`, with
   `readiness_detail` explaining a `false`. Every 10–15 s is fine; a create takes a few minutes, and a
   failure shows `error_message` with `ready: false`. `readiness_detail.stuck` (nested inside
   `readiness_detail`, not a top-level field) turns true once provisioning has run 15 minutes
   without becoming ready
   (`app/serializers/cloud_instance_serializer.rb:109`, `app/models/cloud_instance.rb:1941`).

   ```sh
   pid=awsinst_0123456789
   until selfhost api /aws/v1/instances/$pid -o json | grep -q '"ready": *true'; do sleep 15; done
   ```

4. Read the connection details. `init_pg_pass` is the ClickHouse password for `username` (default user
   `default`); the internal superuser password is never exposed (`app/serializers/cloud_instance_serializer.rb:45,96-98`).
   Connect on `dns_name` (`host`) at `effective_port` — 8123, or 9090 while CHProxy is enabled
   (`app/models/cloud_instance.rb:1736`).

5. Tear down. **Destructive.** `create_snapshot: true` is not an EBS snapshot for ClickHouse: the
   pre-delete strategy is `:preserve_native_backups`, so completed native backups survive
   (`app/services/database_adapters/clickhouse_adapter.rb:66`, `instances_controller.rb:1431`).

   ```sh
   echo '{"pids":["awsinst_0123456789"]}' | selfhost api --input - /aws/v1/instances/delete
   ```

## Day-2 operations

- **Stop / start** act on a whole group: pass the master's pid and its replicas come along; a
  replica-only pid is refused (`instances_controller.rb:1822,1901`). **Reboot** only loops masters —
  replicas in the pid list are silently skipped, and a batch with no master is refused
  (`instances_controller.rb:2016`). `start` re-checks billing after
  `insufficient_credit`/`unpaid_pause` (`instances_controller.rb:1950`).
- **Update** is one PATCH, but not everything applies before the response. Inside the request:
  `identifier` → `name`, `db_port`, `public_access`/`allowed_cidr_ranges`, and
  `backup_enabled`/`dlm_policy_config` alone (`instances_controller.rb:2300,2316-2320,2532,2582,2657`). `tags`,
  `delete_protection`, `instance_type` and `storage` go to `UpdateInstanceJob`, leaving the row at
  `updating` — an immediate re-read shows no change and no error (`app/jobs/update_instance_job.rb:230,236,351`; `instances_controller.rb:2818`).
- **Replicas (HA)**: the same PATCH with `multi_az` plus `replica_count`. The gate is on the total
  (existing live replicas plus the requested count), which must be at least 1 — so the param itself
  may be 0 or blank when a live replica already exists (billable). A real cluster with
  `keeper_hosts`, not a bolt-on (`instances_controller.rb:2435-2440`).
- **Resize** (`PUT …/scale`): single-node and keeper-only instances resize in place; multi-AZ data
  planes use the provision+switchover chain (`app/services/database_adapters/clickhouse_adapter.rb:125`). Storage-only scaling is
  always available.
- **Failover** (`POST …/trigger_failover`) needs master, `multi_az`, status `running` or
  `configuring_multi_az`, a running
  replica, no pending tasks and a paid plan, and is cooldown-limited (`instances_controller.rb:3411-3484`);
  `rpo_override` needs a recent `failover_quorum_denied` entry (`instances_controller.rb:3493`).
- **Users**: create/update/rotate/delete need `running`/`provisioned` and a paid plan; `rotate_password`
  reveals the new password once at `data.meta.new_password` (`database_users_controller.rb:31-75,159`).
- **CHProxy** needs a ClickHouse instance in `running`/`provisioned` with an agent. Ranges:
  `max_concurrent_queries` 1–2000, `max_queue_size` 0–10000, `max_queue_time` 0–3600 s,
  `max_execution_time` 0–86400 s (`chproxy_controller.rb:148-165,178-208`).
- **Backups**: one `clickhouse_native` backup per instance at a time, to a bucket provisioned on demand
  for the instance (`app/services/clickhouse/native_backup_dispatch_service.rb:21,54,108-113`); for
  object-storage instances native restore — not the EBS
  snapshot — is the durable recovery path.
- **Snapshots**: `create_snapshot` bills snapshot storage, needs a provisioned instance, and records
  status `in_progress` (not `completed`) because EC2 only requests it (`volumes_controller.rb:99-102`).
- **Backup policies**: `policy_type=clickhouse_native` is a DB-only row (no DLM object), one per
  instance — a second create is a 422 (`backup_policies_controller.rb:611`).

## Gotchas

- **Nested params and arrays are JSON-only.** `storage`, `database_user` and `backup` are
  `params.require(...)` (`app/validators/aws_validators/instances_validator.rb:91`, `database_users_controller.rb:264`,
  `backups_controller.rb:54`); `pids: []`, `databases: []`, `tags: [{key,value}]` need `--input -`.
- **Tags change shape between create and update.** Create takes an array of `{key,value}`; PATCH takes
  a flat hash and rejects the array form with an explicit 422 (`instances_controller.rb:2062`).
- **Multi-AZ ClickHouse needs `replica_count` at create**, ≥ 1, or a 422
  (`instances_controller.rb:190-196`).
- **Object storage is the default.** Omitting `storage_mode` behaves like `object_storage`; pass
  `storage_mode=local` for EBS-only. A blank `s3_bucket` is auto-provisioned per instance; `s3_endpoint`
  and `s3_prefix` are honored only for a user-supplied bucket (`instances_controller.rb:3561-3582`).
- **Snapshot restore of an object-storage cluster is gated.** The source cluster must not be active and
  the snapshot must sit inside the data bucket's 30-day noncurrent-version window, else 422 pointing
  at a native backup (`instances_controller.rb:475-505`).
- **Native RESTORE is additive.** A failed full-scope restore cannot be retried into the same target
  without `restore_cleanup_confirmed: true` or a fresh target (a scoped `databases` retry is exempt), and
  only a completed backup restores into a running, agent-backed target (`backups_controller.rb:313-368,370-385,857`).
- **`init_pg_pass` is returned on the read only.** Create and fork omit it
  (`instances_controller.rb:1159`); the create-time override is `custom_password`.
- **`default` is the bootstrap user** and cannot be deleted; usernames cannot be renamed in place
  (only the access role, expiry and password-rotation flag update in place — a connection limit is
  refused on ClickHouse), and `rotate_password` refuses engine-reserved names (`database_users_controller.rb:184,19,228`).
  Deleting a non-bootstrap engine-reserved row succeeds and just removes the platform record
  (nothing to delete on the instance); only the bootstrap admin row is refused
  (`database_users_controller.rb:198`).
- **`db_version` is pinned to 26.8.** A PATCH sending the current version is a no-op echo; any other
  value is refused — create a new instance and restore or migrate (`instances_controller.rb:2256-2280`).
- **Failover exposes no phase list.** The gate uses `FAILOVER_PROTECTED_STATES` (`promoting`,
  `fenced_stopping`, `fenced_terminated`, `failover_blocked`, …), and the tail record lives in `engine_config.failover_swap.phase` (`app/models/cloud_instance.rb:97-121,825`).
- **Keeper nodes exist.** A multi-AZ cluster creates `keeper_only` arbiters: no customer data, no
  `db_stats` (`db_stats_controller.rb:37`), and a single-node instance with no backups is unrecoverable (`app/models/cloud_instance.rb:931`).
- **`public_access` defaults true, and no CIDRs means open.** With `public_access` unset and no
  `allowed_cidr_range(s)` key, create adds `0.0.0.0/0` on the default port and only logs it
  server-side. `network_note` comes back only when an explicit empty `allowed_cidr_range(s)` was sent
  (the VPC-only case) and says the database is reachable only inside its VPC
  (`instances_controller.rb:878,1173,3880`).
- **Kafka Table Topics are not for ClickHouse.** `/aws/v1/instances/{id}/table_topics…`
  (`config/routes.rb:369-375`) answers 422 "Table Topics are only supported for Kafka instances." (`table_topics_controller.rb:152`).
- **`GET /aws/v1/snapshots` is account-wide**, not org-filtered — the service lists every snapshot the
  account owns (`app/services/cloud_provider/aws/aws_backup_service.rb:439`, via `snapshots_controller.rb:20`).
- **Rate limits**: create 10/min per user, stop/start/reboot/update/scale 10/min, delete 5/min, fork
  3/min — 429, one retry, then exit 75 (`instances_controller.rb:92-104`).
- **Plan gates differ.** ClickHouse users, CHProxy, stats and logs answer 422 "Top up your balance…"
  from `require_paid_plan!`, and `clickhouse users list` is ungated (`database_users_controller.rb:9`).
  Backups have no `require_paid_plan!` at all: `POST /aws/v1/backups` calls `require_billable!` (402
  "Top up your balance to create this resource."), while list/restore/delete run for an unpaid org
  (`backups_controller.rb:87`, `app/controllers/concerns/billing_gate.rb:100`).

## Sources

- `config/routes.rb`: `:328-346` lifecycle, `:348-349` users, `:362-366` logs/stats, `:369-375` table_topics,
  `:384` chproxy, `:413-416` backups, `:419-421` volumes, `:425` snapshots, `:427` backup_policies,
  `:435-445` regions/pricing.
- Controllers (`app/controllers/aws/v1/`): `instances_controller.rb`, `database_users_controller.rb`,
  `chproxy_controller.rb`, `backups_controller.rb`, `snapshots_controller.rb`,
  `backup_policies_controller.rb`, `logs_controller.rb`, `db_stats_controller.rb`, `volumes_controller.rb`,
  `table_topics_controller.rb`, `regions_controller.rb`, `pricing_controller.rb`.
- `app/serializers/`: `cloud_instance_serializer.rb`, `database_user_serializer.rb`,
  `chproxy_config_serializer.rb`, `backup_serializer.rb`. `app/models/`: `cloud_instance.rb`,
  `database_user.rb`. `app/services/`: `database_adapters/clickhouse_adapter.rb`,
  `clickhouse_keeper_adapter.rb`, `clickhouse/native_backup_dispatch_service.rb`;
  `app/validators/aws_validators/instances_validator.rb`.
