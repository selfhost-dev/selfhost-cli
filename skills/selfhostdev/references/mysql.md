# MySQL

Two surfaces share this name. Family A: a MySQL instance on AWS EC2, created through
`/aws/v1/instances` with `type_of_dbms=mysql` — provisioning, lifecycle, users, engine tuning,
ProxySQL, logs, stats, PITR, backups and snapshots. Family B: a managed MySQL container inside a
Coolify project at `/api/v1/platform/projects/:project_id/databases/mysql` (far smaller).

Every typed verb answers `not implemented yet: mysql …` (`src/cli/mysql.rs`), so use `selfhost api`
with the paths below. Pids: instance `awsinst_<20 hex>` (`cloud_instance.rb:2160-2166`), project
`prj_<ulid>` (`coolify_project.rb:326-327`), project database `prj_my_<ulid>`
(`coolify_database.rb:114-126`), database user `dbu_…`.

## Before you start

- `selfhost auth status` must pass; exit 3 means sign in (`selfhost auth login`).
- The **AWS family** resolves the org from the `organization_id` query/body parameter
  (`application_controller.rb:112-143`; `organization_pid` is an alias), so a resolved `--org` — or
  the CLI's automatic injection — covers every `/aws/v1` call. The **Coolify family** derives the org
  from the project's `organization_pid` (`databases_controller.rb:220`).
- Rules: `references/api.md`. Leading-slash path; a GET that carries parameters needs `-X GET` (a
  bare `-f` turns it into a POST); `-f` sends raw strings, `-F` types `true`/`false`/`null`/numbers.
  `key[sub]` is a literal key, so nested JSON (`storage`, `backup`, `mysql_config`, `database_user`)
  and arrays (`pids`) have no flag form — send them with `--input -`. Billable (402/exit 4) and
  destructive calls are marked below.

## Endpoint map

### Cloud instances — `/aws/v1/instances`

| Future CLI command | Method and path | What it does | Key parameters | Source |
|---|---|---|---|---|
| `mysql list` | `GET /aws/v1/instances/by_organization` | The org's instances, grouped by `group_id` into `instances.master` / `instances.replicas` | — | `config/routes.rb:330`, `instances_controller.rb:1307` |
| `mysql show` / `wait` | `GET /aws/v1/instances/:pid` | One instance (delegates to `details`); poll `data[0].ready` / `data[0].status` | `pid` (path) | `config/routes.rb:346`, `instances_controller.rb:1221,1304` |
| `mysql metrics` | `GET /organizations/:organization_id/instance_metrics` | Heartbeat series for one instance | `pid`, `metric_type=mysql.database`, `start_date`, `end_date` | `config/routes.rb:263`, `organizations_controller.rb:263`, `mysql_adapter.rb:26-28` |
| `mysql create` | `POST /aws/v1/instances` | **Billable.** Provision and start async provisioning | `type_of_dbms=mysql`, `region`, `identifier`, `instance_type`, `storage{type,size,iops,throughput}`, optional `db_version`, `username`, `multi_az`, `public_access`, `backup_enabled`, `delete_protection` | `config/routes.rb:328`, `instances_controller.rb:108`, `instances_validator.rb:46-51` |
| `mysql update` | `PATCH /aws/v1/instances/:pid` | `delete_protection`, `public_access`, `allowed_cidr_ranges`, `backup_enabled`, `pitr_enabled`, `db_port`, `tags` | permit list | `config/routes.rb:336`, `instances_controller.rb:2045`, `:2070-2107` |
| `mysql start` / `stop` / `reboot` | `PUT /aws/v1/instances/start` \| `/stop` \| `/reboot` | Start, stop or reboot a group | `pids` (array of instance pids) | `config/routes.rb:333-335`, `instances_controller.rb:1822,1901,1990` |
| `mysql resize` / `scale` | `PUT /aws/v1/instances/:pid/scale` | Instance-type resize and/or volume growth | `target_instance_type`, `new_size_gb`, `new_iops`, `new_throughput`, `new_volume_type` | `config/routes.rb:338`, `instances_controller.rb:3151` |
| `mysql fork` | `POST /aws/v1/instances/:pid/fork` | **Billable.** Clone via EBS snapshot into a new single node | `instance_name` (required), `cloud_credential_id` | `config/routes.rb:337`, `instances_controller.rb:2845`, `:2868-2879` |
| `mysql failover` | `POST /aws/v1/instances/:pid/trigger_failover` | Promote a replica; multi-AZ + paid plan only | `rpo_override` (optional) | `config/routes.rb:339`, `instances_controller.rb:3411`, `:3424-3428` |
| `mysql delete` | `POST /aws/v1/instances/delete` | **Destructive.** Soft-delete the listed instances | `pids` (array), optional `create_snapshot` | `config/routes.rb:331`, `instances_controller.rb:1346` |
| `mysql refresh` | `POST /aws/v1/instances/refresh_by_group_id` | Re-read a whole group from AWS and return the refreshed instances; 404 when `group_id` is not in your org | `group_id` | `config/routes.rb:332`, `instances_controller.rb:3363-3372` |

### Database users — `/aws/v1/instances/:instance_id/database_users`

| Future CLI command | Method and path | What it does | Key parameters | Source |
|---|---|---|---|---|
| `mysql users list` | `GET /aws/v1/instances/:instance_id/database_users` | Users not yet deleted | — | `config/routes.rb:348`, `database_users_controller.rb:27` |
| `mysql users create` | `POST /aws/v1/instances/:instance_id/database_users` | Create a user; async agent sync, starts `pending`; no password echoed | body `{"database_user":{…}}` — `username`, `password`, `role` required; optional `connection_limit`, `target_database`, `expires_at`, `password_rotation_enabled` | `config/routes.rb:348`, `database_users_controller.rb:35,264`, `:12` |
| `mysql users update` | `PATCH /aws/v1/instances/:instance_id/database_users/:id` | Change `role`, `connection_limit`, `expires_at`, `password_rotation_enabled` | body `{"database_user":{…}}` — any of those; username/password are ignored | `config/routes.rb:348`, `database_users_controller.rb:89,264`, `:19` |
| `mysql users rotate-password` | `POST /aws/v1/instances/:instance_id/database_users/:id/rotate_password` | Generate a new password, returned once | — | `config/routes.rb:349`, `database_users_controller.rb:132` |
| `mysql users delete` | `DELETE /aws/v1/instances/:instance_id/database_users/:id` | Delete a user | — | `config/routes.rb:348`, `database_users_controller.rb:167` |

Roles `read_only`, `read_write`, `admin` (`database_user.rb:7`); statuses `pending`, `active`,
`failed`, `deleting`, `deleted` (`:8`); fields `database_user_serializer.rb:7,9-35`. A flat body 422s
"Missing database_user payload" (`database_users_controller.rb:12,264`).

### Per-instance engine config — `mysql_config`

| Future CLI command | Method and path | What it does | Key parameters | Source |
|---|---|---|---|---|
| `mysql config show` | `GET /aws/v1/instances/:pid/mysql_config` | The tunables that have a persisted row | `pid` (path) | `config/routes.rb:410`, `mysql_configs_controller.rb:8` |
| `mysql config set` | `PATCH /aws/v1/instances/:pid/mysql_config` | Override parameters; enqueues an agent task, sets the instance to `updating` | body `{"mysql_config":{"<param>":<value>}}` | `config/routes.rb:411`, `mysql_configs_controller.rb:40`, `:94-96` |
| `mysql config restart-required` | `GET /aws/v1/instances/:pid/mysql_config` | Per-parameter `restart_required`; a PATCH also returns the top-level flag | — | `mysql_config.rb:5`, `mysql_configs_controller.rb:65` |

Parameters: `max_connections`, `innodb_buffer_pool_size`, `tmp_table_size`, `wait_timeout`
(`mysql_config.rb:4`); the first two need a restart (`:5`).

### Connection pool — ProxySQL (`mysql pool *` maps here; client port 6033)

| Future CLI command | Method and path | What it does | Key parameters | Source |
|---|---|---|---|---|
| `mysql pool show` | `GET /aws/v1/instances/:instance_id/proxysql` | Config; 200 with `{proxysql: null}` when never enabled | — | `config/routes.rb:383`, `proxysql_controller.rb:17` |
| `mysql pool enable` | `POST /aws/v1/instances/:instance_id/proxysql` | Enable (async, `enabling`) | `max_connections`, `multiplexing`, `connection_max_age_ms` | `config/routes.rb:383`, `proxysql_controller.rb:27`, `:176` |
| `mysql pool update` | `PUT /aws/v1/instances/:instance_id/proxysql` | Reconfigure; requires it already enabled | same | `config/routes.rb:383`, `proxysql_controller.rb:67` |
| `mysql pool disable` | `DELETE /aws/v1/instances/:instance_id/proxysql` | Disable | — | `config/routes.rb:383`, `proxysql_controller.rb:105` |

### Logs and stats

| Future CLI command | Method and path | What it does | Key parameters | Source |
|---|---|---|---|---|
| `mysql logs` | `GET /aws/v1/instances/:instance_id/logs` | Recent fetches (20) | — | `config/routes.rb:362`, `logs_controller.rb:28` |
| `mysql logs` (fetch) | `POST /aws/v1/instances/:instance_id/logs` | Dispatch an agent log tail; returns the `pending` fetch | `log_source`, `lines` (default 200, max 2000) | `config/routes.rb:362`, `logs_controller.rb:42,56,61` |
| `mysql stats` | `GET /aws/v1/instances/:instance_id/db_stats` | Recent stats fetches (20) | — | `config/routes.rb:366`, `db_stats_controller.rb:19` |
| `mysql stats` (sample) | `POST /aws/v1/instances/:instance_id/db_stats` | Dispatch a stats snapshot | `sample_seconds` (default 10, 5..60) | `config/routes.rb:366`, `db_stats_controller.rb:29-30` |

MySQL sources (`GET .../logs/sources`, `config/routes.rb:363`): `engine`, `walg_mysql`,
`walg_restore`, `replication` (`mysql_adapter.rb:133-144`). Fetch rows carry `status` (`pending` →
`completed`/`failed`), `output`, `error` (`database_log_fetch_serializer.rb:2-3`); stats rows carry
`stats` (`database_stats_fetch_serializer.rb:2-3`).

### PITR — `/aws/v1/instances/:instance_id/pitr`

MySQL **does** support PITR (`mysql_adapter.rb:30-32`).

| Future CLI command | Method and path | What it does | Key parameters | Source |
|---|---|---|---|---|
| `mysql pitr status` | `GET /aws/v1/instances/:instance_id/pitr` | Config, or `{pitr: null}` when not configured | — | `config/routes.rb:352`, `pitr_controller.rb:18` |
| `mysql pitr enable` | `POST /aws/v1/instances/:instance_id/pitr` | Enable WAL archiving to S3, async (`configuring` → `enabled`) | `retention_days` (required, positive integer) | `config/routes.rb:352`, `pitr_controller.rb:34,55-56`, `:635-646` |
| `mysql pitr configure` | `PUT /aws/v1/instances/:instance_id/pitr/configure` | Change retention (applied by the next base backup) | `retention_days` (optional) | `config/routes.rb:357`, `pitr_controller.rb:117` |
| `mysql pitr pause` / `resume` | `POST /aws/v1/instances/:instance_id/pitr/pause` \| `/resume` | Suspend/resume base backups; WAL archiving continues | — | `config/routes.rb:358-359`, `pitr_controller.rb:174,215` |
| `mysql pitr retry` | `POST /aws/v1/instances/:instance_id/pitr/retry` | Retry a config stuck in `error` | — | `config/routes.rb:356`, `pitr_controller.rb:358` |
| — | `GET`/`POST /aws/v1/instances/:instance_id/pitr/base_backups` \| `/base_backup` | List base backups newest-first, any status (`pending`/`running`/`completed`/`failed`); take one now (needed before any restore) | — | `config/routes.rb:353-354`, `pitr_controller.rb:297,312`, `pitr_base_backup.rb:2,15` |
| `mysql pitr restore` | `POST /aws/v1/instances/:instance_id/pitr/restore` | **Billable.** Provisions a *new* instance restored to a timestamp | `target_time` (ISO 8601, required), `name` (required), optional `instance_type`, `storage_type`, `storage_size` | `config/routes.rb:355`, `pitr_controller.rb:412,434-435` |

### Backups, snapshots, backup policies — siblings of `instances`, never nested under it

| Future CLI command | Method and path | What it does | Key parameters | Source |
|---|---|---|---|---|
| `mysql backups list` | `GET /aws/v1/backups` | The org's backup records | `organization_id` | `config/routes.rb:413`, `backups_controller.rb:22` |
| `mysql backups create` | `POST /aws/v1/backups` | **Billable.** EBS snapshot backup through AWS Backup — needs an existing backup vault plus a resolvable execution role and resource ARN | `backup{backup_type=aws_backup_job, resource_id, organization_id, region, options{backup_vault_name (required — the vault must already exist), iam_role_arn, resource_arn}}` | `config/routes.rb:413`, `backups_controller.rb:50,54-60,128-158` |
| `mysql backups delete` | `DELETE /aws/v1/backups/:id` | **Destructive.** Delete a backup record | — | `config/routes.rb:413`, `backups_controller.rb:538` |
| `mysql backups restore` | — | Not on this endpoint (native ClickHouse/Kafka only) — restore with `POST /aws/v1/instances` + `snapshot_id` | — | `config/routes.rb:416`, `backups_controller.rb:306-310` |
| `mysql snapshots list` | `GET /aws/v1/snapshots` | **Account-wide** EBS snapshots in one region, not instance-scoped | `region`, `cloud_credential_id` | `config/routes.rb:425`, `snapshots_controller.rb:12-20` |
| `mysql snapshots create` | `POST /aws/v1/volumes/create_snapshot` | **Billable.** Snapshot one volume of an instance (`data` default or `root`; `all` is not usable and fails) | `pid` (instance), `organization_id`, `volume_type` (`data` default, `root`) | `config/routes.rb:421`, `volumes_controller.rb:12-18,59`, `aws_backup_service.rb:573-581` |
| `mysql snapshots delete` | `DELETE /aws/v1/snapshots/:id` | **Destructive.** Delete an EBS snapshot | `id` (AWS snapshot id) | `config/routes.rb:425`, `snapshots_controller.rb:103` |
| `mysql snapshots restore` | — | No endpoint: create an instance with `snapshot_id` | — | `instances_controller.rb:427` |
| — | `POST /aws/v1/backup_policies`; `GET /aws/v1/backup_policies`; `PATCH`/`DELETE /aws/v1/backup_policies/:id` | **Billable.** DLM schedule: create, list, update, delete (list is collection-only — there is no per-policy read; create always starts `ENABLED`) | create: `organization_id`, optional `cloud_instance_pid`, `policy_name`, `description`, `target_tags`, `schedule_interval`, `schedule_unit`, `retain_rule_count`, `region`; update only: `state`, `policy_details{…}` | `config/routes.rb:427`, `backup_policies_controller.rb:253,454-490,19,33-34,145,44,344,947-962` |

### Regions and pricing — public, no sign-in required

| Future CLI command | Method and path | What it does | Key parameters | Source |
|---|---|---|---|---|
| — | `GET /aws/v1/pricing` | The pricing catalog for a region | `region` (optional) | `config/routes.rb:444`, `pricing_controller.rb:13-17` |
| — | `POST /aws/v1/pricing/estimate` | Monthly estimate before you create, priced exactly like the bill | `region`, `instance_type`, `storage_type`, `storage_size_gb`, `db_type=mysql`, optional `mode` (`managed` default, `byoc`), `replica_count`, `database_count`, `public_ipv4_count`, `iops`, `throughput_mbps` | `config/routes.rb:445`, `pricing_controller.rb:22-23,35-49`, `hybrid_pricing_service.rb:22-62,269-288` |

Both skip authentication and the organization requirement, and both rate-limit to 60 per minute
(`pricing_controller.rb:6-11`). An unknown mode, region, instance type or storage type comes back
as 422 (`hybrid_pricing_service.rb:208-230`, rendered by `pricing_controller.rb:29-31`); the
catalog raises its own on a region it has no prices for (`:143-149`).

### Managed project database — `/api/v1/platform/projects/:project_id/databases`

| Future CLI command | Method and path | What it does | Key parameters | Source |
|---|---|---|---|---|
| `mysql list` (project) | `GET /api/v1/platform/projects/:project_id/databases` | Every database of one Coolify project, newest first — MySQL and other engines alike; pick the MySQL ones out yourself | — | `config/routes.rb:37`, `databases_controller.rb:30-35` |
| `mysql show` (project) | `GET /api/v1/platform/projects/:project_id/databases/:pid` | One database **including the connection password** | — | `config/routes.rb:38`, `databases_controller.rb:45` |
| `mysql create` (project) | `POST /api/v1/platform/projects/:project_id/databases/mysql` | Declare a managed MySQL container; 202 async (no billing gate — only project creation is billable) | `name`, `mysql_root_password`, `mysql_password`, `mysql_user`, `mysql_database`, `is_public`, `public_port`, `instant_deploy` (accepted but ignored — stripped before provisioning and never read) | `config/routes.rb:45`, `databases_controller.rb:87`, `mysql/database_validator.rb:9-25` |
| — | `POST` then `GET .../databases/:pid/logs` (also `.../stats`) | Container log tail and stats: POST dispatches, GET polls | `lines` | `config/routes.rb:41-44`, `container_inspectable.rb:28-31` |
| `mysql delete` (project) | `DELETE /api/v1/platform/projects/:project_id/databases/:pid` | **Destructive.** Soft-delete now, Coolify cleanup async | `name` (must equal the display name) | `config/routes.rb:46`, `databases_controller.rb:167,171-176` |

## Provision one end to end

1. Pick a region and instance type: `GET /aws/v1/regions` and
   `GET /aws/v1/regions/:region_code/instance_types` (`config/routes.rb:435-436`). MySQL versions
   are `8.0` (the default) and `8.4` (`mysql_adapter.rb:22-23`, `base.rb:45-47`). To see what it
   will cost first, ask for an estimate with `db_type=mysql` and your region, instance type and
   storage (see the pricing table above; no sign-in needed).
2. Create. **Billable.** The nested `storage` object needs a raw body:

```sh
echo '{"type_of_dbms":"mysql","region":"us-east-1","identifier":"shop-db",
  "instance_type":"t3.medium","db_version":"8.0","username":"appuser",
  "storage":{"type":"gp3","size":20,"iops":3000,"throughput":125},
  "public_access":true,"delete_protection":false}' | selfhost api /aws/v1/instances --input -
```

The response is 200 with `data: { organization_id, group_id, pid, username, credentials_path, host }`
(`instances_controller.rb:1152-1177`) — **no password and no instance object**.
3. Poll `GET` the `credentials_path` value. `data` is an array: watch `data[0].status` and
   `data[0].ready` (`instances_controller.rb:1304`). `status` walks
   `pending → provisioning → provisioned → running` (`cloud_instance.rb:1384`); for a single
   instance or master, `ready` flips true only when `running` + `internal_status == "completed"`
   + a blank `error_message` + a healthy agent (`cloud_instance.rb:1853-1861`). Replicas use a
   narrower check instead — `running` + `completed` + an intact replication link, with no
   `error_message` or agent-health check. Failure shows `status: "failed"` with `error_message`:

```sh
selfhost api /aws/v1/instances/awsinst_REPLACE_ME -o json --silent || echo "not ready" >&2
```

4. Read the connection from `data[0]`: `username` (default `admin`, `base.rb:342-344`),
   `init_pg_pass` (`:45`) — the engine's **root** password, which the agent hands it as
   `MYSQL_ROOT_PASSWORD`, and the one that pairs with `username` for your own connection
   (`mysql_adapter.rb:168-169,181-183`). Also `public_dns` / `dns_name` (`:67,35`),
   `database_name` (`mysql`), and `effective_port` — 3306, or 6033 under ProxySQL
   (`cloud_instance.rb:1736-1742`). The platform's separate admin account is a different secret:
   it lives in `shdev_admin_pass`, is encrypted to the agent, and is never part of any response
   (`cloud_instance_credential_serializer.rb:31-34,63-66`), so `init_pg_pass` is the only
   password you ever get back.
5. Tear down. **Destructive:**

```sh
echo '{"pids":["awsinst_REPLACE_ME"]}' | selfhost api /aws/v1/instances/delete --input -
```

## Day-2 operations

- **start/stop expand to the group** (every replica in the master's `group_id`,
  `instances_controller.rb:1846-1849,1925-1928`); **reboot does not** — it iterates only the masters
  you pass (`:2016`). All three return only a message (`:1890,1979,2034`); re-poll until `ready`.
- **resize/scale**: `PUT .../scale`; MySQL uses the provision+switchover chain
  (`mysql_adapter.rb:10-12`). The response gives `data.scaling_event_id`; the instance payload later
  carries `active_scale_event` / `last_scale_event` (`cloud_instance_serializer.rb:150-180`).
- **fork / failover**: fork is an EBS-snapshot clone, and a multi-AZ source always forks to a single
  node (`instances_controller.rb:3126-3129`); failover needs multi-AZ, a paid plan, a running replica
  and status `running`/`configuring_multi_az`.
- **users**: create returns no password — keep the one you sent; `rotate-password` is the only
  reveal-once path (`data.meta.new_password`, `database_users_controller.rb:159`).
- **config / pooler**: PATCH only the four parameters; `max_connections` and
  `innodb_buffer_pool_size` set the top-level `restart_required`. `mysql pool *` is ProxySQL (client
  6033, admin 6032, `proxysql_config.rb:5-6`); while enabled, `effective_port` is 6033.
- **backups / PITR**: `POST /aws/v1/backups` (`backup_type=aws_backup_job`) snapshots the volume;
  restore by creating an instance with `snapshot_id`, whose engine tag must match
  (`instances_controller.rb:589-596`). PITR needs `retention_days` and a **completed base backup**
  before `restore`, which provisions a new instance.

## Gotchas

- **`dashboards` is not MySQL.** `GET /aws/v1/instances/:pid/dashboards` answers 422 "Dashboards is
  only supported for OpenSearch instances" (`instances_controller.rb:3387-3389`).
- **Path-parameter names differ.** `:pid` for the `GET /instances/:pid` read and `mysql_config`;
  `:id` for the update, scale, fork and trigger_failover member routes; `:instance_id` for the
  nested sub-resources (`database_users`, `pitr`, `logs`, `db_stats`, `proxysql`). Rails matches
  position, not name, so the values work the same either way.
- **Create needs `region`, `identifier`, `instance_type`, `storage`** and a registered `type_of_dbms`
  (`instances_validator.rb:46-51,83-88`). `storage.type` ∈ `gp2 gp3 io1 io2 st1 sc1 standard`; gp3
  needs `iops >= 3000`, io1/io2 `iops >= 100` and `size >= 4`, HDD types `size >= 125` (`:96-142`);
  for gp2/st1/sc1/standard the validator silently zeroes `iops`/`throughput`. A snapshot restore
  (`snapshot_id` set, `instance_type` blank) skips it (`instances_controller.rb:80,4185-4187`).
- **Reserved usernames.** `username` must match `\A[a-z_][a-z0-9_-]{0,30}\z`, not end in `-`, and not
  be `selfhostadmin`, `selfhost_superuser`, `selfhost_replication`, `replicator` or start with `pg_`
  (`instances_validator.rb:55-80`); database users have their own list (`database_user.rb:27-40`).
- **`mysql_config` takes a `:pid` and a nested body.** Only the four parameters are accepted; `show`
  omits parameters with no persisted row (`mysql_configs_controller.rb:17-19`). An empty
  `mysql_config` object passes the "required" guard and enqueues a no-op task (`:54,94-96`), and
  `restart_required` is computed from the request's keys, so clearing an override with `null` still
  reports it (`:65`).
- **ProxySQL needs MySQL *and* a paid plan** — the paid-plan gate is unscoped (covers even
  `mysql pool show`, so a non-billable org gets 422 on a read), while the MySQL-only check runs on
  create/update/destroy only, so a read of a non-MySQL row would not fail on that basis
  (`proxysql_controller.rb:12-14,149-152,169-173`). Settable: `max_connections` (1..100000),
  `multiplexing` (boolean), `connection_max_age_ms` (>= 0); `default_hostgroup` is read-only
  (`:176,179-199`). `mysql pool update` is a `PUT`.
- **Backups and snapshots are not instance-scoped.** A backup's instance is the body parameter
  `resource_id` (an instance pid), a volume snapshot's is `pid`, and `GET /aws/v1/snapshots` lists the
  **entire AWS account's** snapshots in a region (`volumes_controller.rb:13-21`,
  `aws_backup_service.rb:432-454`).
- **Only `aws_backup_job` applies here.** Create accepts `aws_backup_job`, `clickhouse_native`,
  `kafka_native` (`backups_controller.rb:15`); `restore` refuses everything but the native types
  (`:306-310`) — hence the MySQL restore path is instance creation with a `snapshot_id`
  (`config/routes.rb:416`). EBS backup policies reject Kafka only
  (`ebs_backup_capability_guard.rb:23-31`).
- **Billing and deletion.** Backup and volume-snapshot creation need the `ebs_snapshot_storage` SKU
  (`backups_controller.rb:86-87`, `volumes_controller.rb:32-33`); instance creation needs its own
  (`instances_controller.rb:637-640`). `delete_protection: true` blocks deletion, a replica needs
  its master in the same `pids` batch, and a master needs all its replicas
  (`instances_controller.rb:1367-1371,1400-1422`).
- **PITR needs a first base backup.** `pitr restore` refuses while `latest_restore_point` is blank
  and points at `POST .../pitr/base_backup` (`pitr_controller.rb:443-452`); `target_time` must be
  ISO 8601, not future, and not before `oldest_restore_point`. Paused configs stay restorable
  (`:424-427`).
- **Coolify family uses `:project_id`, not `:project_pid`** (`databases_controller.rb:213`). Create
  takes `mysql_root_password`/`mysql_password` (>= 8 chars, and no spaces or `` ` `` `$` `;` `|` `&`
  `<` `>` `\` `'` `"`), `mysql_user`/`mysql_database` identifiers and `name` <= 255; `is_public`
  defaults to true, so `public_port` (1024..65535) is effectively required on every default create
  unless the caller explicitly sends `is_public=false` (`mysql/database_validator.rb:35,70-73`).
  Destroy wants the display name (422 otherwise) and is a soft delete plus an async job returning
  202 (`databases_controller.rb:180-189`).
- **Coolify credentials and status.** `GET .../databases/:pid` passes `include_credentials: true`, so
  its `connection` object carries the password; the list does not (`:73-76`,
  `coolify_database.rb:58-70`). Create returns 202 with `status` `creating` (or `pending` while the
  host comes up) plus a `provisioning` payload; create 409s on three distinct causes — a port clash
  (`databases_controller.rb:121`), the capacity guard (`:128`), and the accepts-children gate
  (`:247-253`) — not just a project that cannot accept children.
- **Rate limits.** Log and stats dispatches 429 (exit 75) during their 10 s / 15 s cooldowns
  (`logs_controller.rb:6,71-72`, `db_stats_controller.rb:10,54-55`); a duplicate dispatch while one
  is pending returns the in-flight record.

## Sources

- `config/routes.rb:37-46` (project databases), `:263` (org instance metrics), `:328-346` (instance
  routes), `:348-366` (`database_users`, `pitr`, `logs`, `db_stats`), `:383` (proxysql), `:410-411`
  (mysql_config), `:413-427` (backups, volumes, snapshots, policies), `:435-436`, `:444-445`
  (pricing catalog and estimate).
- Controllers `app/controllers/aws/v1/{instances,database_users,mysql_configs,proxysql,backups,volumes,snapshots,backup_policies,logs,db_stats,pitr,pricing}_controller.rb`,
  `api/v1/platform/databases_controller.rb`, `organizations_controller.rb`,
  `concerns/container_inspectable.rb`; serializers `cloud_instance_serializer.rb`,
  `cloud_instance_credential_serializer.rb`,
  `database_user_serializer.rb`, `proxysql_config_serializer.rb`,
  `database_log_fetch_serializer.rb`, `database_stats_fetch_serializer.rb`,
  `pitr_configuration_serializer.rb`.
- Models/services `cloud_instance.rb`, `coolify_database.rb`, `coolify_project.rb`, `pitr_base_backup.rb`,
  `mysql_config.rb`, `proxysql_config.rb`, `database_user.rb`,
  `app/services/database_adapters/{mysql_adapter,base,registry}.rb`,
  `app/services/mysql/database_validator.rb`, `app/services/database_provisioning/mysql_strategy.rb`,
  `app/services/pricing/hybrid_pricing_service.rb`,
  `app/services/cloud_provider/aws/aws_backup_service.rb`,
  `app/validators/aws_validators/instances_validator.rb`, `concerns/ebs_backup_capability_guard.rb`.
