# PostgreSQL

Two provisioning families. **Cloud instances** (`/aws/v1/instances`, `type_of_dbms=postgres`) are
AWS EC2 boxes you own end to end: create, fork, scale, users, config, PgBouncer, extensions, PITR,
backups, snapshots, logs, stats. **Managed project databases**
(`/api/v1/platform/projects/:project_id/databases`, `db_type=postgresql`) are Coolify containers on
a project VM: create, show, logs, stats, destroy only. Every `selfhost postgres …` verb answers
`not implemented yet`, so drive the paths below with `selfhost api`. Instances are addressed by `pid`;
a cluster is a `group_id` (`grp_…`) with one master and N replicas. Cloud instances resolve
their organization server-side, so the CLI's injected `organization_id` scopes them; managed
project databases skip that step and derive the org from the project pid instead, ignoring
the injected `organization_id` (`app/controllers/api/v1/platform/databases_controller.rb:9`).

## Before you start

- `selfhost auth status` must be signed in; `selfhost org use <slug>` (or `--org`) picks the org.
- Read [api.md](api.md). Three rules bite here: a bare `-f`/`-F` turns the call into a POST, so a GET
  carrying parameters needs `-X GET`; `-f` values stay strings while `-F` types booleans/integers, and
  neither can express a nested object or array (use `--input -`); `organization_id` is injected for you.

```sh
selfhost api -X GET /aws/v1/instances/by_organization -o json   # instances in your org
selfhost api -X PUT /aws/v1/instances/stop -f pids=awsinst_<id>      # PUT needs -X
echo '{"pids":["awsinst_<id>"]}' | selfhost api /aws/v1/instances/delete --input -
```

## Endpoint map

### Cloud instances (`type_of_dbms=postgres`)

| Future CLI command | Method and path | What it does | Key parameters | Source |
|---|---|---|---|---|
| `postgres list` | `GET /aws/v1/instances/by_organization` | every instance in the org, grouped by `group_id` into `master` / `replicas` | — | `config/routes.rb:330`, `app/controllers/aws/v1/instances_controller.rb:1307` |
| `postgres show` / `postgres wait` | `GET /aws/v1/instances/:pid` | one instance + `metrics`, `task_progress`, `ready`, wrapped as a one-element `data` array — read `data[0]`; poll it until `ready` is true | `pid` | `config/routes.rb:346`, `app/controllers/aws/v1/instances_controller.rb:1221`, `app/serializers/cloud_instance_serializer.rb:109` |
| `postgres show` (alt) | `GET /aws/v1/instances/details` | same serializer, also wrapped as a one-element `data` array; also resolves an instance from `snapshot_id` | `pid`, `instance_id`, `snapshot_id` | `config/routes.rb:329`, `app/controllers/aws/v1/instances_controller.rb:1225` |
| `postgres create` | `POST /aws/v1/instances` | provision master (+ replicas when `multi_az`); billable | `type_of_dbms`, `identifier`, `region`, `instance_type`, `storage{type,size,iops,throughput}`, `db_version`, `multi_az`, `replica_count`, `extensions[]`, `postgresql_config{}`, `pitr_enabled`, `custom_password`, `public_access`, `allowed_cidr_ranges[]`, `vpc_id`, `tags[]`, `no_auto_failover_ack` | `config/routes.rb:328`, `app/controllers/aws/v1/instances_controller.rb:108,109`, `app/validators/aws_validators/instances_validator.rb:46` |
| `postgres update` | `PATCH /aws/v1/instances/:pid` | day-2 fields, incl. `identifier`, `delete_protection`, `public_access`, `multi_az`, `backup_enabled`, `db_port`, `data_durability`, `sync_number`, `extensions[]`, `tags{}`, `tls_enabled` (alone). `db_version` is refused on PostgreSQL (only a ClickHouse row reaches that branch, and only to be told in-place version changes are not offered), and `rpo_override` is accepted but ignored — it only takes effect on the failover endpoint | `pid` | `config/routes.rb:336`, `app/controllers/aws/v1/instances_controller.rb:2045,2070,2257` |
| `postgres delete` | `POST /aws/v1/instances/delete` | destroy the listed pids; destructive; `create_snapshot` takes a final EBS snapshot | `pids[]`, `create_snapshot` | `config/routes.rb:331`, `app/controllers/aws/v1/instances_controller.rb:1346` |
| `postgres stop` / `start` / `reboot` | `PUT /aws/v1/instances/stop`, `…/start`, `…/reboot` | group stop, start, and a plain EC2 reboot (the broker-readiness probe the reboot primitive dispatches only applies to Kafka rows, so PostgreSQL reboots keep their existing behaviour); a replica without its master in `pids` is refused | `pids[]` | `config/routes.rb:333`, `config/routes.rb:334`, `config/routes.rb:335`, `app/controllers/aws/v1/instances_controller.rb:1822,1901,1990`, `app/services/cloud_provider/aws/aws_instance_rebooter_service.rb:24` |
| `postgres fork` | `POST /aws/v1/instances/:pid/fork` | new instance from the source's EBS snapshot; billable | `instance_name` (required), `cloud_credential_id` | `config/routes.rb:337`, `app/controllers/aws/v1/instances_controller.rb:2845` |
| `postgres resize` / `postgres scale` | `PUT /aws/v1/instances/:pid/scale` | one endpoint: `target_instance_type` resizes, `new_size_gb`/`new_iops`/`new_throughput`/`new_volume_type` scales storage | `pid` | `config/routes.rb:338`, `app/controllers/aws/v1/instances_controller.rb:3151` |
| `postgres failover` | `POST /aws/v1/instances/:pid/trigger_failover` | promote a replica; master only, `multi_az` only, paid plan | `rpo_override` | `config/routes.rb:339`, `app/controllers/aws/v1/instances_controller.rb:3411` |
| `postgres refresh` (can write) | `POST /aws/v1/instances/refresh_by_group_id` | re-reads a whole cluster from AWS EC2 and writes the answer back, not a pure read: a live row gets its status re-derived from the EC2 state and its sync timestamp bumped, and the response is the refreshed rows through the same instance serializer as the other reads. A row EC2 no longer returns is stamped `status: terminated` and `is_deleted: true` with a customer-visible activity entry, so a stale AWS id silently retires a database — destructive. A `group_id` outside your org answers 404 "Instance group not found"; a multi-AZ master mid-failover is left alone apart from its sync timestamp, and rows in a failover state or already deleted are skipped entirely and never appear in the answer. No paid-plan gate, just org membership | `group_id` (required) | `config/routes.rb:332`, `app/controllers/aws/v1/instances_controller.rb:3363,3366`, `app/services/cloud_provider/aws/group_data_refresh_service.rb:23,34,70,155,168`

### Database users

| Future CLI command | Method and path | What it does | Key parameters | Source |
|---|---|---|---|---|
| `postgres users list` | `GET /aws/v1/instances/:instance_id/database_users` | live users (deleted rows hidden) | — | `config/routes.rb:348`, `app/controllers/aws/v1/database_users_controller.rb:27` |
| `postgres users create` | `POST /aws/v1/instances/:instance_id/database_users` | create a role; the row stays `pending` until the agent syncs | `database_user{username, password, role}` (required), plus `connection_limit`, `target_database`, `expires_at`, `password_rotation_enabled` | `config/routes.rb:348`, `app/controllers/aws/v1/database_users_controller.rb:35,263` |
| `postgres users update` | `PATCH /aws/v1/instances/:instance_id/database_users/:id` | only `role`, `connection_limit`, `expires_at`, `password_rotation_enabled` | `database_user{}` | `config/routes.rb:348`, `app/controllers/aws/v1/database_users_controller.rb:89,19` |
| `postgres users delete` | `DELETE /aws/v1/instances/:instance_id/database_users/:id` | soft-delete + agent drop; the bootstrap admin is refused | — | `config/routes.rb:348`, `app/controllers/aws/v1/database_users_controller.rb:167` |
| `postgres users rotate-password` | `POST /aws/v1/instances/:instance_id/database_users/:id/rotate_password` | new password, returned once as `meta.new_password` | — | `config/routes.rb:349`, `app/controllers/aws/v1/database_users_controller.rb:132,159` |
All four mutating user actions need a paid plan — free-plan orgs get 422; only the list read is ungated (`app/controllers/aws/v1/database_users_controller.rb:9`).

### Config tuning

| Future CLI command | Method and path | What it does | Key parameters | Source |
|---|---|---|---|---|
| `postgres config show` / `restart-required` | `GET /aws/v1/instances/:pid/postgresql_config` | the nine tunables with `calculated_value` / `override_value` / `effective_value` / `restart_required` | — | `config/routes.rb:407`, `app/controllers/aws/v1/postgresql_configs_controller.rb:83` |
| `postgres config set` | `PATCH /aws/v1/instances/:pid/postgresql_config` | apply overrides; `null` removes one; restarts the node when a restart-required param changes | `postgresql_config{ <param>: value }` | `config/routes.rb:408`, `app/controllers/aws/v1/postgresql_configs_controller.rb:115` |
| `postgres config preview` | `GET /aws/v1/postgresql_configs/preview` | calculated values for an instance type + region, with optional overrides | `instance_id` **or** (`instance_type`, `region`), `overrides[]` (each an object with `parameter_name` (or `name`) plus `value`; anything else is silently dropped) | `config/routes.rb:404`, `app/controllers/aws/v1/postgresql_configs_controller.rb:14` |

Tunables: `shared_buffers`, `effective_cache_size`, `random_page_cost`, `work_mem`,
`maintenance_work_mem`, `max_connections`, `effective_io_concurrency`, `log_min_duration_statement`,
`checkpoint_completion_target`; `shared_buffers` and `max_connections` need a restart (`app/models/postgresql_config.rb:4,16`).

### PgBouncer pool

| Future CLI command | Method and path | What it does | Key parameters | Source |
|---|---|---|---|---|
| `postgres pool show` | `GET /aws/v1/instances/:instance_id/pgbouncer` | the group's pooler (`owned_by_this_instance` says whether it runs here); paid plan | — | `config/routes.rb:382`, `app/controllers/aws/v1/pgbouncer_controller.rb:27` |
| `postgres pool enable` | `POST /aws/v1/instances/:instance_id/pgbouncer` | start the pooler on port 6432; paid plan | `pool_mode`, `max_client_conn`, `default_pool_size`, `min_pool_size`, `reserve_pool_size` | `config/routes.rb:382`, `app/controllers/aws/v1/pgbouncer_controller.rb:52` |
| `postgres pool update` | `PUT /aws/v1/instances/:instance_id/pgbouncer` | reconfigure a pooler this instance owns; paid plan | same five | `config/routes.rb:382`, `app/controllers/aws/v1/pgbouncer_controller.rb:92` |
| `postgres pool disable` | `DELETE /aws/v1/instances/:instance_id/pgbouncer` | drain and stop; only while `enabled`/`error`; paid plan | — | `config/routes.rb:382`, `app/controllers/aws/v1/pgbouncer_controller.rb:130` |
The paid-plan gate is unconditional: show, update and disable answer 422 on free-plan orgs too (`app/controllers/aws/v1/pgbouncer_controller.rb:12`).

`pool_mode` is `session`, `transaction` or `statement` (`app/models/pgbouncer_config.rb:5`).

### Extensions

| Future CLI command | Method and path | What it does | Key parameters | Source |
|---|---|---|---|---|
| `postgres extensions catalog` | `GET /aws/v1/postgres_extensions` | the catalog with no instance in play, split into `available` / `unavailable` | `db_version`, `architecture` | `config/routes.rb:443`, `app/controllers/aws/v1/postgres_extensions_controller.rb:34` |
| `postgres extensions list` | `GET /aws/v1/instances/:instance_id/extensions` | `enabled` rows on this instance plus `available` / `unavailable` for its version and arch | — | `config/routes.rb:380`, `app/controllers/aws/v1/instance_extensions_controller.rb:23` |
| `postgres extensions enable` | `POST /aws/v1/instances/:instance_id/extensions` | install on every group node; 202 when at least one extension starts, 200 with `started: []` when every requested extension is already enabled or in progress | `extensions[]` (or `name`) | `config/routes.rb:380`, `app/controllers/aws/v1/instance_extensions_controller.rb:29` |

Catalog names: `pgvector`, `timescaledb`, `pgcrypto`, `uuid-ossp`, `postgres_fdw`, `pg_partman`,
`pg_stat_statements`, `pg_trgm`, `pg_cron`, `postgis`, `pg_textsearch`
(`app/services/postgres_extensions/catalog.rb:204`); there is no disable endpoint.

### Pricing (public — no org needed)

Both pricing routes skip authentication and the org lookup entirely, so you can quote a cluster
before you have one or before you have picked an org; they share a 60 requests/minute rate limit
(`app/controllers/aws/v1/pricing_controller.rb:6,10`).

| Future CLI command | Method and path | What it does | Key parameters | Source |
|---|---|---|---|---|
| `postgres price catalog` | `GET /aws/v1/pricing` | the monthly catalog for a region: currency, what the monthly price includes versus bills separately, the list of regions that actually have pricing data (not every region AWS supports), and per instance type an `hourly_price` plus a 730-hour `monthly_price`; the answer is not wrapped in a one-element array | `region` (defaults to `us-east-1`); an unknown region or one with no reference data answers 422 | `config/routes.rb:444`, `app/controllers/aws/v1/pricing_controller.rb:13`, `app/services/pricing/hybrid_pricing_service.rb:143,195` |
| `postgres price estimate` | `POST /aws/v1/pricing/estimate` | monthly quote for one cluster shape. `mode=managed` (the default) is cost plus margin and itemises compute, storage, the margin, and any public IPv4 addresses. `mode=byoc` is 30% of the managed price per database with no infrastructure charge on your side, and reports the managed equivalent, the per-database price, and the total database count instead — it has no IPv4 line and never charges for public addresses. IOPS and throughput are optional: omit them and the storage type's own baseline is priced, pass them and the excess over that baseline is billed. Every bad or out-of-range value (unknown region, unsupported instance type or storage type, size, IOPS, or throughput outside the type's range, a count that is not a whole number or is below its minimum) answers 422 with the reason, and nothing is created | `region`, `instance_type`, `storage_type`, `storage_size_gb` (all required), `mode`, `db_type` (not validated), `database_count` (min 1), `replica_count`, `public_ipv4_count`, `iops`, `throughput_mbps` | `config/routes.rb:445`, `app/controllers/aws/v1/pricing_controller.rb:22,35`, `app/services/pricing/hybrid_pricing_service.rb:22,66,208,387` |

The catalog reports a full 730-hour month with no proration (`app/services/pricing/hybrid_pricing_service.rb:155`);
the estimate uses the same 730-hour basis when it turns hourly rates into monthlies
(`app/services/pricing/hybrid_pricing_service.rb:12,127`). Both answer in USD.
The quote reads the same SKU rows and margin table the real charge path uses, so it tracks a repricing
or a margin override rather than a hardcoded markup (`app/services/pricing/hybrid_pricing_service.rb:48`).
Like any GET that takes parameters, the catalog read needs the explicit method when you pass
`region` on the command line, or it turns into a POST.

### PITR

| Future CLI command | Method and path | What it does | Key parameters | Source |
|---|---|---|---|---|
| `postgres pitr status` | `GET /aws/v1/instances/:instance_id/pitr` | config with `status`, `retention_days`, `oldest_restore_point`, `latest_restore_point`, `s3_uri` | — | `config/routes.rb:352`, `app/controllers/aws/v1/pitr_controller.rb:18` |
| `postgres pitr enable` | `POST /aws/v1/instances/:instance_id/pitr` | turn on WAL archiving; the S3 bucket is resolved server-side | `retention_days` (required) | `config/routes.rb:352`, `app/controllers/aws/v1/pitr_controller.rb:34` |
| `postgres pitr configure` | `PUT /aws/v1/instances/:instance_id/pitr/configure` | change retention; applied by the next base backup | `retention_days` (optional) | `config/routes.rb:357`, `app/controllers/aws/v1/pitr_controller.rb:117` |
| `postgres pitr pause` | `POST /aws/v1/instances/:instance_id/pitr/pause` | suspend scheduled base backups; archives stay | — | `config/routes.rb:358`, `app/controllers/aws/v1/pitr_controller.rb:174` |
| `postgres pitr resume` | `POST /aws/v1/instances/:instance_id/pitr/resume` | resume, with a catch-up base backup if stale | `retention_days` (optional) | `config/routes.rb:359`, `app/controllers/aws/v1/pitr_controller.rb:215` |
| `postgres pitr retry` | `POST /aws/v1/instances/:instance_id/pitr/retry` | re-dispatch a config stuck in `error` | — | `config/routes.rb:356`, `app/controllers/aws/v1/pitr_controller.rb:358` |
| `postgres pitr restore` | `POST /aws/v1/instances/:instance_id/pitr/restore` | provision a NEW instance restored to a point in time; billable | `target_time` (ISO 8601), `name` (required), `instance_type`, `storage_type`, `storage_size` | `config/routes.rb:355`, `app/controllers/aws/v1/pitr_controller.rb:412` |
| `postgres pitr base-backups` (list / run) | `GET` then `POST /aws/v1/instances/:instance_id/pitr/base_backups` / `…/base_backup` | recent base backups; trigger one now, which needs the config status exactly `enabled` (refused after a pause or while still configuring) | — | `config/routes.rb:353`, `config/routes.rb:354`, `app/controllers/aws/v1/pitr_controller.rb:297,312` |
| `postgres pitr disable` | `DELETE /aws/v1/instances/:instance_id/pitr` | disable + detach the S3 grant; archives are left in place | — | `config/routes.rb:352`, `app/controllers/aws/v1/pitr_controller.rb:265` |
Every PITR action, including the status read, needs a paid plan — free-plan orgs get 422 (`app/controllers/aws/v1/pitr_controller.rb:11`).

### Backups, snapshots, backup policies

| Future CLI command | Method and path | What it does | Key parameters | Source |
|---|---|---|---|---|
| `postgres backups list` | `GET /aws/v1/backups` | org backups (syncs from AWS first) | `organization_id` | `config/routes.rb:413`, `app/controllers/aws/v1/backups_controller.rb:22` |
| `postgres backups create` | `POST /aws/v1/backups` | start an AWS Backup job; billable | `backup{backup_type, resource_id, organization_id, region, options{}}`; `aws_backup_job` needs `options{backup_vault_name}`, a resolvable execution role (`options{iam_role_arn}`, the AWSBackupDefaultServiceRole in the target account, or the org's default instance IAM role), and a full resource ARN — without a role it 422s before any Backup row is written | `config/routes.rb:413`, `app/controllers/aws/v1/backups_controller.rb:50,54,128` |
| `postgres backups delete` | `DELETE /aws/v1/backups/:id` | soft-delete + delete the snapshot in AWS; destructive | — | `config/routes.rb:413`, `app/controllers/aws/v1/backups_controller.rb:538` |
| `postgres backups restore` | `POST /aws/v1/backups/:id/restore` | **not a PostgreSQL path** — rejects anything but `clickhouse_native` / `kafka_native` | `target_instance_id` | `config/routes.rb:416`, `app/controllers/aws/v1/backups_controller.rb:289,308` |
| `postgres snapshots create` | `POST /aws/v1/volumes/create_snapshot` | EBS snapshot of the instance volume; billable | `pid`, `organization_id`, `volume_type` (`data` default; `root` answers success but takes no snapshot — `data.snapshot_id` comes back null, so use `data` for anything you want to restore), `cloud_credential_id` | `config/routes.rb:421`, `app/controllers/aws/v1/volumes_controller.rb:12,13` |
| `postgres snapshots list` / `show` | `GET /aws/v1/snapshots`, `GET /aws/v1/snapshots/:id` | every snapshot the AWS account owns in the region (account-wide, not org-scoped); `:id` is an AWS snapshot id | `organization_id` (membership gate only), `region` (default `us-east-1`), `cloud_credential_id` | `config/routes.rb:425`, `app/controllers/aws/v1/snapshots_controller.rb:12,49,159` |
| `postgres snapshots restore` | `POST /aws/v1/instances` with `snapshot_id` | restore = create a new instance from the snapshot; billable | `snapshot_id` (+ `type_of_dbms`, `region`, `identifier`, `storage`) | `config/routes.rb:328`, `app/controllers/aws/v1/instances_controller.rb:427,589` |
| `postgres snapshots delete` | `DELETE /aws/v1/snapshots/:id` | delete the AWS snapshot; destructive | `cloud_credential_id` | `config/routes.rb:425`, `app/controllers/aws/v1/snapshots_controller.rb:103` |
| `postgres backups policy` | `GET /aws/v1/backup_policies` | DLM policies for the org | `organization_id`, `cloud_instance_pid` | `config/routes.rb:427`, `app/controllers/aws/v1/backup_policies_controller.rb:19` |
| `postgres backups policy` (create) | `POST /aws/v1/backup_policies` | create a DLM EBS-snapshot policy; billable | `organization_id`, `region`, `policy_name`, `description`, `target_tags[]`, `schedule_interval`, `schedule_unit`, `retain_rule_count`, `cloud_instance_pid`, `execution_role_arn` | `config/routes.rb:427`, `app/controllers/aws/v1/backup_policies_controller.rb:253,454` |
| `postgres backups policy` (update) | `PATCH /aws/v1/backup_policies/:id` | change schedule or state | `organization_id`, `region`, `description`, `state`, `execution_role_arn`, `policy_details{}` | `config/routes.rb:427`, `app/controllers/aws/v1/backup_policies_controller.rb:145,947` |
| `postgres backups policy` (delete) | `DELETE /aws/v1/backup_policies/:id` | delete the policy; destructive | `organization_id`, `region`, `cloud_credential_id` | `config/routes.rb:427`, `app/controllers/aws/v1/backup_policies_controller.rb:44` |

### Logs, stats, metrics

| Future CLI command | Method and path | What it does | Key parameters | Source |
|---|---|---|---|---|
| `postgres logs sources` | `GET /aws/v1/instances/:instance_id/logs/sources` | Postgres sources: `engine`, `walg_archive`, `walg_restore`, `replication` | — | `config/routes.rb:363`, `app/controllers/aws/v1/logs_controller.rb:17`, `app/services/database_adapters/postgres_adapter.rb:151` |
| `postgres logs` (dispatch / poll) | `POST`, then `GET /aws/v1/instances/:instance_id/logs` | tail a source; paid plan, 10 s cooldown; poll last 20 fetches for `status` + `output` | `log_source`, `lines` (default 200, max 2000) | `config/routes.rb:362`, `app/controllers/aws/v1/logs_controller.rb:42,188,28` |
| `postgres stats` (dispatch / poll) | `POST`, then `GET /aws/v1/instances/:instance_id/db_stats` | sample engine query stats (15 s cooldown); poll last 20 fetches for `status` + `stats` | `sample_seconds` (default 10, 5–60) | `config/routes.rb:366`, `app/controllers/aws/v1/db_stats_controller.rb:30,89,19` |
| `postgres metrics` | `GET /organizations/{org}/instance_metrics` | org-level metric series; pass an instance `pid` to scope it | `pid`, `start_date`, `end_date` (both required) | `config/routes.rb:263`, `app/controllers/organizations_controller.rb:263,276` |

### Managed project database (Coolify)

| Future CLI command | Method and path | What it does | Key parameters | Source |
|---|---|---|---|---|
| `project db list` | `GET /api/v1/platform/projects/:project_id/databases` | every database in the project (all engines) | — | `config/routes.rb:37`, `app/controllers/api/v1/platform/databases_controller.rb:30` |
| `project db show` | `GET /api/v1/platform/projects/:project_id/databases/:pid` | one database; the only place `connection.password` appears | — | `config/routes.rb:38`, `app/controllers/api/v1/platform/databases_controller.rb:45,75` |
| `project db create` | `POST /api/v1/platform/projects/:project_id/databases/postgresql` | declare a PostgreSQL container; `extensions` accepted **here only** | `name`, `postgres_user`, `postgres_password`, `postgres_db`, `public_port`, `is_public`, `instant_deploy`, `extensions[]` | `config/routes.rb:45`, `app/services/postgres/database_validator.rb:9,27,40` |
| `project db logs` | `POST`, then `GET /api/v1/platform/projects/:project_id/databases/:pid/logs` | dispatch then poll, like services | — | `config/routes.rb:41`, `config/routes.rb:42` |
| `project db stats` | `POST`, then `GET /api/v1/platform/projects/:project_id/databases/:pid/stats` | dispatch then poll | — | `config/routes.rb:43`, `config/routes.rb:44` |
| `project db delete` | `DELETE /api/v1/platform/projects/:project_id/databases/:pid` | soft-delete now, Coolify cleanup async; destructive | `name` must equal the display name | `config/routes.rb:46`, `app/controllers/api/v1/platform/databases_controller.rb:167,171` |

The image follows the requested extensions (`app/services/database_provisioning/postgresql_strategy.rb:13,107`):
`postgres:17-alpine` is the default and is never sent in the request; a request naming one specialised
extension uses that extension's catalog image (for example `pgvector/pgvector:pg17` or
`timescale/timescaledb:latest-pg17`), and the bundle image is only picked as the cheapest single image
covering the full requested set.

### Verbs with no dedicated endpoint

- `postgres tls enable|disable` → `PATCH /aws/v1/instances/:pid` `{"tls_enabled":true|false}`,
  sent **alone** and applied group-wide (`app/controllers/aws/v1/instances_controller.rb:2325`).
  `postgres tls verify` has no route: read `ca_certificate` and `tls_expires_at` off
  `GET /aws/v1/instances/:pid` (`app/serializers/cloud_instance_serializer.rb:299-300`).
- `postgres durability set` → `PATCH /aws/v1/instances/:pid` with `data_durability`
  (`async`|`preferred`|`required`), optional `sync_number` and `no_auto_failover_ack`
  (`app/models/cloud_instance.rb:284,286`).
- `postgres replicas list` → `GET /aws/v1/instances/by_organization`; replicas sit under their
  `group_id`. `replicas create` → `POST /aws/v1/instances` with `multi_az: true` (+ `replica_count`),
  or `PATCH {"multi_az":true}` on the master. `replicas delete` → `POST /aws/v1/instances/delete` with
  the master **and** every replica pid. No replica-specific route exists.

## Provision one end to end

```sh
# 1. pick a region and instance type (public reference data, org not needed)
selfhost api /aws/v1/regions
selfhost api /aws/v1/regions/eu-central-1/instance_types

# 2. create. BILLABLE. `storage` is required; `db_version` omitted defaults to "14".
cat <<'JSON' | selfhost api /aws/v1/instances --input -
{
  "type_of_dbms": "postgres",
  "identifier": "orders-pg",
  "region": "eu-central-1",
  "instance_type": "t4g.medium",
  "db_version": "17",
  "storage": { "type": "gp3", "size": 50, "iops": 3000, "throughput": 125 },
  "multi_az": false,
  "extensions": ["pg_stat_statements"],
  "backup_enabled": true,
  "custom_password": "<pick-a-password>"
}
JSON
# → data: { pid, group_id, username, host, credentials_path }
```

3. Poll `GET /aws/v1/instances/<pid>` every 10–15 s until `data[0].ready` is `true`. On a master or
   singleton, readiness needs `status: running`, `internal_status: completed`, a blank
   `error_message` and a fresh agent heartbeat; a replica is ready when running and completed
   with its replication link intact, without the error-message or heartbeat checks.
   `readiness_detail` names what still blocks, and a failure surfaces as `status: "failed"`
   or a populated `error_message` (`app/serializers/cloud_instance_serializer.rb:109`,
   `app/models/cloud_instance.rb:1853,1932`).

4. Read the connection details from the same call: the password is `init_pg_pass`, the port is
   `effective_port` (5432, or 6432 once PgBouncer is on) and `database_name` is `postgres`
   (`app/serializers/cloud_instance_serializer.rb:118`).

5. Tear down — **destructive**. There is no `DELETE /aws/v1/instances/:pid`; deletion is a POST.

```sh
echo '{"pids":["awsinst_<pid>"],"create_snapshot":true}' \
  | selfhost api /aws/v1/instances/delete --input -
```

## Day-2 operations

| Task | Call |
|---|---|
| stop / start / reboot | `PUT /aws/v1/instances/{stop,start,reboot}` `{"pids":[…]}` |
| resize type, disk or IOPS | `PUT /aws/v1/instances/:pid/scale` — `{"target_instance_type":"m7g.large"}`, or `{"new_size_gb":200,"new_iops":6000,"new_throughput":250}` |
| fork / failover | `POST /aws/v1/instances/:pid/fork` `{"instance_name":"orders-pg-copy"}`, or `…/trigger_failover` |
| tune config | `PATCH /aws/v1/instances/:pid/postgresql_config` `{"postgresql_config":{"work_mem":"16MB"}}` |
| enable an extension | `POST /aws/v1/instances/:pid/extensions` `{"extensions":["pg_cron"]}` |
| enable PgBouncer | `POST /aws/v1/instances/:pid/pgbouncer` `{"pool_mode":"transaction","default_pool_size":20}` |
| enable PITR, then base-backup | `POST /aws/v1/instances/:pid/pitr` `{"retention_days":7}`, then `POST …/pitr/base_backup` |
| snapshot now / on a schedule | `POST /aws/v1/volumes/create_snapshot` `{"pid":"awsinst_…","organization_id":"org_…"}`, or `POST /aws/v1/backup_policies` with a DLM schedule |
| enable TLS / set durability | `PATCH /aws/v1/instances/:pid` `{"tls_enabled":true}` or `{"data_durability":"required"}` |
| force a live status read of the cluster | `POST /aws/v1/instances/refresh_by_group_id` `{"group_id":"grp_…"}` — creates nothing, but can mark a row terminated if EC2 no longer knows it |

## Gotchas

- **`type_of_dbms=postgres`, not `postgresql`.** The managed project path uses `db_type=postgresql`;
  mixing them fails validation (`app/validators/aws_validators/instances_validator.rb:86`).
- **Pricing does not validate `db_type`.** Pass `postgres`, `postgresql`, or leave it out and a
  PostgreSQL quote comes back the same — the value only reaches the margin calculation
  (`app/services/pricing/hybrid_pricing_service.rb:270,277`), so unlike the create call it will not
  tell you the spelling is wrong.
- **`db_version` defaults to `14`**, the first of `supported_versions` (`%w[14 15 16 17 18]`) — pass
  `"17"` explicitly (`app/services/database_adapters/postgres_adapter.rb:20`, `app/services/database_adapters/base.rb:45`).
- **Delete is `POST /aws/v1/instances/delete`** with `pids[]`; a master cannot go without all its
  replicas in the same batch, and `delete_protection` blocks it
  (`app/controllers/aws/v1/instances_controller.rb:1374,1406`).
- **Snapshot restore is instance creation.** `POST /aws/v1/backups/:id/restore` refuses PostgreSQL —
  pass `snapshot_id` to `POST /aws/v1/instances`. When the snapshot's originating record resolves,
  its engine and version are silently forced onto the new instance; the "snapshots are
  engine-specific" rejection only fires when the origin cannot be resolved and the snapshot's
  own tags disagree with the request
  (`app/controllers/aws/v1/instances_controller.rb:511-513,583-588`).
- **A fresh snapshot is not instantly restorable.** `create_snapshot` records it `in_progress`; AWS
  reports it `pending` for minutes and `ReconcileBackupStatusJob` flips it to `completed` — wait for
  that before restoring (`app/controllers/aws/v1/volumes_controller.rb:99`).
- **PITR restore needs a completed base backup.** `latest_restore_point` is set only by a finished
  `pitr_base_backup`, so `POST /pitr/restore` 422s until you run `base_backup` and wait. The config
  must be `enabled` or `paused`, and `target_time` must be parseable ISO 8601, not in the future, not
  older than `oldest_restore_point` (`app/controllers/aws/v1/pitr_controller.rb:449,456,466`).
- **PITR enable requires `retention_days`**; configure/resume reuse the recorded window and pause
  ignores it. A retention change lands with the *next* base backup
  (`app/controllers/aws/v1/pitr_controller.rb:163,639`).
- **Credentials appear once.** `init_pg_pass` is readable from `GET /aws/v1/instances/:pid` (left out
  of the create response on purpose) and a rotated password arrives only as `data.meta.new_password`.
  Never echo either into logs, files or a commit
  (`app/controllers/aws/v1/instances_controller.rb:1164`, `app/controllers/aws/v1/database_users_controller.rb:159`).
- **`tls_enabled` must be the only field in its PATCH** — a mixed body 422s
  (`app/controllers/aws/v1/instances_controller.rb:2326`).
- **PgBouncer belongs to the group.** `show` resolves the group's pooler and sets
  `owned_by_this_instance`; `PUT`/`DELETE` are refused on a member that does not own it
  (`app/controllers/aws/v1/pgbouncer_controller.rb:27,92`).
- **Rate limits** (per user, or user+IP): create 10/min, destroy 5/min, fork 3/min,
  update/stop/start/reboot/scale 10/min, config preview/show/update 20/min; a 429 exits 75 (`app/controllers/aws/v1/instances_controller.rb:92,96,100,104`,
  `app/controllers/aws/v1/postgresql_configs_controller.rb:10` for the config endpoints).
- **The `-X GET` trap.** `selfhost api /aws/v1/snapshots -f region=eu-central-1` is a POST with a JSON
  body; add `-X GET` to keep it on the query string.

## Sources

- `config/routes.rb:263,326-445` (AWS v1), `:33,37-46` (platform API).
- Controllers: `app/controllers/aws/v1/{instances,database_users,postgresql_configs,pgbouncer,instance_extensions,postgres_extensions,pricing,pitr,backups,volumes,snapshots,backup_policies,logs,db_stats}_controller.rb`,
  `app/controllers/api/v1/platform/databases_controller.rb`, `app/controllers/organizations_controller.rb`.
- Serializers: `cloud_instance_serializer.rb`, `cloud_instance_credential_serializer.rb`, `database_user_serializer.rb`, `pgbouncer_config_serializer.rb`, `pitr_configuration_serializer.rb`,
  `pitr_base_backup_serializer.rb`, `database_log_fetch_serializer.rb`, `database_stats_fetch_serializer.rb`.
- Models/services: `cloud_instance.rb`, `pgbouncer_config.rb`, `postgresql_config.rb`,
  `coolify_database.rb`, `database_adapters/{postgres_adapter,base}.rb`,
  `database_provisioning/postgresql_strategy.rb`, `postgres/database_validator.rb`,
  `postgres_extensions/catalog.rb`, `validators/aws_validators/instances_validator.rb`.
- Services: `app/services/pricing/hybrid_pricing_service.rb`,
  `app/services/cloud_provider/aws/group_data_refresh_service.rb`.
