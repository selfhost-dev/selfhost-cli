# OpenSearch

Managed OpenSearch clusters on AWS. Every `selfhost opensearch …` verb — the
instance verbs plus `users`, `snapshots`, `backups`, `dashboards` — answers
`not implemented yet: opensearch <verb>`, so drive the raw API with `selfhost
api` and the paths below. A cluster is a *group* of `CloudInstance` rows: one
endpoint node (`role: "master"`, `opensearch_endpoint: true`) plus HA siblings
(`role: "replica"`), all sharing a `group_id`, with `awsinst_<20 hex>` pids
(`app/models/cloud_instance.rb:2160-2166`). Everything is
org-scoped through the resolved organization (`organization_id`), never a path.

## Before you start

- `selfhost auth status` — a signed-in profile, and an organization resolved
  (`--org <slug>` or `selfhost org use <slug>`). The CLI appends the resolved
  organization **pid** as `organization_id` to every request; the controllers
  match instances on `pid` + `orgs_pid` (`app/controllers/aws/v1/instances_controller.rb:1257`,
  `app/controllers/application_controller.rb:122`).
- Read `references/api.md`. What bites here: `-X GET` for any GET carrying
  parameters (`-f`/`-F` alone make it a POST); nested bodies need `--input`
  (raw JSON), because `-f storage.type=gp3` is one literal key; 402 is exit 4
  and 429 is exit 75 after one retry.
- Versions are fixed by the adapter: `3.8` (default, newest 3.x patch) and
  `2.19` (`app/services/database_adapters/opensearch_adapter.rb:43`). No endpoint
  lists them; anything else is a 422 (`app/models/cloud_instance.rb:2100`).
- Default port `9200` (`app/services/database_adapters/opensearch_adapter.rb:17`); `effective_port` always
  equals `db_port` for OpenSearch (the payload carries `db_port`, not `port`).

## Endpoint map

### Instances

| Future CLI command | Method and path | What it does | Key parameters | Source |
|---|---|---|---|---|
| `selfhost opensearch list` | `GET /aws/v1/instances/by_organization` | Read-only. Every non-deleted instance in the org, grouped by `group_id` into `instances.master` / `instances.replicas`. No engine or pagination filter — filter `type_of_dbms` yourself | none | `config/routes.rb:330`, `app/controllers/aws/v1/instances_controller.rb:1307` |
| `selfhost opensearch show` | `GET /aws/v1/instances/:pid` | Read-only. Canonical single read (#2799). `data` is an **array of one**: serializer fields + `metrics` + `task_progress` | path `pid` | `config/routes.rb:346`, `app/controllers/aws/v1/instances_controller.rb:1221-1304` |
| (alternative) | `GET /aws/v1/instances/details` | Read-only, same body; also accepts `instance_id` or `snapshot_id` | `pid`, `instance_id`, `snapshot_id` | `config/routes.rb:329`, `app/controllers/aws/v1/instances_controller.rb:1225-1257` |
| `selfhost opensearch create` | `POST /aws/v1/instances` | Mutating, **billable**. Region/type/RAM/quota checks, price SKU, master row, `ProvisionInstanceJob`. `data` = `organization_id`, `pid`, `group_id`, `username`, `host`, `credentials_path` — **no password** | see recipe | `config/routes.rb:328`, `app/controllers/aws/v1/instances_controller.rb:108-1184` |
| `selfhost opensearch delete` | `POST /aws/v1/instances/delete` | **Destructive** (EC2 terminate + soft-delete). `create_snapshot=true` takes pre-delete EBS snapshots first | `pids` (master **and all replicas**) | `config/routes.rb:331`, `app/controllers/aws/v1/instances_controller.rb:1346,1808` |
| `selfhost opensearch start` | `PUT /aws/v1/instances/start` | Mutating, group-wide: pass the master's pid. Balance-gated after a credit stop | `pids` | `config/routes.rb:334`, `app/controllers/aws/v1/instances_controller.rb:1901-1979` |
| `selfhost opensearch stop` | `PUT /aws/v1/instances/stop` | Mutating, group-wide (replicas first, then master) | `pids` | `config/routes.rb:333`, `app/controllers/aws/v1/instances_controller.rb:1822-1890` |
| `selfhost opensearch reboot` | `PUT /aws/v1/instances/reboot` | Mutating. Master only, and only while `running` | `pids` | `config/routes.rb:335`, `app/controllers/aws/v1/instances_controller.rb:1990-2034` |
| `selfhost opensearch update` | `PATCH /aws/v1/instances/:pid` | Mutating. Tags are an object here (`{"env":"staging"}`), not the create-time array. Enqueues `UpdateInstanceJob` | `identifier`, `tags{}`, `public_access`, `delete_protection`, `allowed_cidr_ranges[]`, `backup_enabled`, `dashboards_enabled`, `instance_type`, `storage{}`, `db_port`, `dlm_policy_config` | `config/routes.rb:336`, `app/controllers/aws/v1/instances_controller.rb:2045-2088,2820` |
| `selfhost opensearch fork` | `POST /aws/v1/instances/:pid/fork` | Mutating, **billable**. Clones the data volume into a NEW group; always `multi_az: false` | `instance_name` (required), `cloud_credential_id` | `config/routes.rb:337`, `app/controllers/aws/v1/instances_controller.rb:2845,3119` |
| `selfhost opensearch resize` | `PUT /aws/v1/instances/:pid/scale` | Mutating, **billable**. Storage-only resizes are solvency-gated too (`app/controllers/aws/v1/instances_controller.rb:3211-3213`). `data.scaling_event_id` is the handle | `target_instance_type`, `new_size_gb`, `new_iops`, `new_throughput`, `new_volume_type` | `config/routes.rb:338`, `app/controllers/aws/v1/instances_controller.rb:3151,3347` |
| `selfhost opensearch scale` | **No endpoint** | The verb exists in the CLI (`--replicas`) but OpenSearch has no API path to change the replica count: `replica_count` is not in the update allowlist, and flipping `multi_az` on an OpenSearch cluster is refused outright. Replica count is fixed at create. Use `resize` for capacity, or create an HA cluster and restore into it | — | `src/cli/opensearch.rs:99`, `app/controllers/aws/v1/instances_controller.rb:2070-2106,2409-2417` |
| `selfhost opensearch failover` | `POST /aws/v1/instances/:pid/trigger_failover` | Mutating. HA master only; paid plan required | optional `rpo_override` | `config/routes.rb:339`, `app/controllers/aws/v1/instances_controller.rb:3411-3522` |
| `selfhost opensearch wait` | `GET /aws/v1/instances/:pid` | Read-only poll of the same read as `show` | — | `config/routes.rb:346`, `app/serializers/cloud_instance_serializer.rb:109-111` |
| (no verb) | `POST /aws/v1/instances/refresh_by_group_id` | Mutating. Re-sync every node of one group from AWS | `group_id` | `config/routes.rb:332`, `app/controllers/aws/v1/instances_controller.rb:3363` |

Response fields (`app/serializers/cloud_instance_serializer.rb:9-90,109-111,188-197`):
`pid`, `group_id`, `role`, `type_of_dbms`, `db_version`, `status`, `internal_status`,
`ready`, `readiness_detail`, `ha_ready`, `error_message`, `instance_type`, `storage_*`,
`region`, `dns_name`, `public_ip`, `private_ip`, `db_port`, `effective_port`, `username`,
`init_pg_pass`, `opensearch_endpoint`, `opensearch_node_role`, `dashboards_*`. `task_progress`
is not a serializer field: the single-instance read adds it, and only when no `snapshot_id`
is passed (`app/controllers/aws/v1/instances_controller.rb:1278`).

### Dashboards

| Future CLI command | Method and path | What it does | Key parameters | Source |
|---|---|---|---|---|
| `selfhost opensearch dashboards show` | `GET /aws/v1/instances/:pid/dashboards` | Read-only. `dashboards_enabled`, `dashboards_status`, `dashboards_url` (`https://<dns_name>:5601`), `dashboards_supported` (>= 4 GB), `dashboards_recommended` (>= 8 GB), `memory_gb` (live AWS lookup) | path `pid` | `config/routes.rb:340`, `app/controllers/aws/v1/instances_controller.rb:3379-3403` |
| `selfhost opensearch dashboards enable` / `disable` | `PATCH /aws/v1/instances/:pid` with `dashboards_enabled` | Mutating. **There is no `POST …/dashboards/enable` route.** Only on the cluster's endpoint node, only in `running`/`provisioned`; enable needs an agent and the 4 GB floor; a no-op flip answers `Already …` immediately. `UpdateInstanceJob` dispatches the install/stop asynchronously | `dashboards_enabled` | `config/routes.rb:336`, `app/controllers/aws/v1/instances_controller.rb:2146-2181`, `app/jobs/update_instance_job.rb:130-148` |

`dashboards_status` ∈ `disabled|pending|installing|running|degraded|failed`
(`app/models/cloud_instance.rb:456`); rebuild paths (restore, fork, resize) reset it to
`pending` so it reinstalls (`app/models/cloud_instance.rb:471-478`).

### Database users

Nested under the instance: `:instance_id` is the **instance pid**, `:id` a `dbu_…` pid (`app/models/database_user.rb:193`). Only the four write verbs need a paid plan — `list` is not gated (`app/controllers/aws/v1/database_users_controller.rb:9`).

| Future CLI command | Method and path | What it does | Key parameters | Source |
|---|---|---|---|---|
| `selfhost opensearch users list` | `GET /aws/v1/instances/:instance_id/database_users` | Read-only, oldest first | — | `config/routes.rb:348`, `app/controllers/aws/v1/database_users_controller.rb:27` |
| `selfhost opensearch users create` | `POST /aws/v1/instances/:instance_id/database_users` | Mutating. `database_user{username,password,role}` — all three required; the row starts `pending`. 201. **`connection_limit` is rejected here too**: a non-null value 422s with `Connection limit is not supported for opensearch instances` and no row is created | `username`, `password`, `role`, `target_database`, `expires_at` | `config/routes.rb:348`, `app/controllers/aws/v1/database_users_controller.rb:35-85,263-265`, `app/models/database_user.rb:46,177-183` |
| `selfhost opensearch users update` | `PATCH /aws/v1/instances/:instance_id/database_users/:id` (PUT routes too) | Mutating. Key **presence** decides what changes, so a `false` flag counts as provided. **`connection_limit` is rejected**: any non-null value 422s with `Connection limit is not supported for opensearch instances`, the row is not saved and the sync job never runs. Sending it as an explicit `null` is the one accepted use — it clears the stored limit | `role`, `expires_at`, `password_rotation_enabled`; `connection_limit` only as an explicit `null` | `config/routes.rb:348`, `app/controllers/aws/v1/database_users_controller.rb:19,89-130`, `app/models/database_user.rb:46,177-183` |
| `selfhost opensearch users delete` | `DELETE /aws/v1/instances/:instance_id/database_users/:id` | Destructive (async). The bootstrap admin cannot be deleted | path `id` | `config/routes.rb:348`, `app/controllers/aws/v1/database_users_controller.rb:167` |
| `selfhost opensearch users rotate-password` | `POST /aws/v1/instances/:instance_id/database_users/:id/rotate_password` | Mutating. Cleartext comes back **once** at `data.meta.new_password` | path `id` | `config/routes.rb:349`, `app/controllers/aws/v1/database_users_controller.rb:132-165` |

User fields: `username`, `role`, `status`, `details`, `created_at`, `updated_at`,
`password_updated_at`, `connection_limit`, `target_database`,
`expires_at`, `password_rotation_enabled`, `instance_pid`, `password_expired`,
`password_locked`, `days_until_password_expiry`, `deletable` (`app/serializers/database_user_serializer.rb:7-34`).

### Logs, stats, metrics

| Future CLI command | Method and path | What it does | Key parameters | Source |
|---|---|---|---|---|
| `selfhost opensearch logs` | `GET /aws/v1/instances/:instance_id/logs` | Read-only. The 20 most recent fetches | — | `config/routes.rb:362`, `app/controllers/aws/v1/logs_controller.rb:28` |
| (first call) | `POST /aws/v1/instances/:instance_id/logs` | Mutating (agent task). Poll the GET above. Valid `log_source` keys are `engine`, `restore`, `fork` and `replication` — the last a grep-filtered view of the engine log (`app/services/database_adapters/opensearch_adapter.rb:239-256`) | `log_source`, `lines` (default 200, max 2000) | `config/routes.rb:362`, `app/controllers/aws/v1/logs_controller.rb:42-118` |
| (no verb) | `GET /aws/v1/instances/:instance_id/logs/sources` | Read-only. Valid `log_source` keys | — | `config/routes.rb:363`, `app/controllers/aws/v1/logs_controller.rb:17` |
| `selfhost opensearch stats` | `GET /aws/v1/instances/:instance_id/db_stats` | Read-only. Recent stat fetches | — | `config/routes.rb:366`, `app/controllers/aws/v1/db_stats_controller.rb:19` |
| (first call) | `POST /aws/v1/instances/:instance_id/db_stats` | Mutating (agent task) | `sample_seconds` (default 10, clamped 5..60) | `config/routes.rb:366`, `app/controllers/aws/v1/db_stats_controller.rb:30-79` |
| `selfhost opensearch metrics` | `GET /aws/v1/instances/:pid` | Read-only. `metrics` comes from the newest agent heartbeat | — | `config/routes.rb:346`, `app/controllers/aws/v1/instances_controller.rb:1273-1279` |

### Backups, volume snapshots, snapshots

| Future CLI command | Method and path | What it does | Key parameters | Source |
|---|---|---|---|---|
| `selfhost opensearch snapshots create` | `POST /aws/v1/volumes/create_snapshot` | Mutating, **billable** (EBS-snapshot SKU). The only way to snapshot on demand; the row starts `in_progress` and a reconciler completes it | `pid` (flat), `volume_type` (default `data`), `organization_id` | `config/routes.rb:421`, `app/controllers/aws/v1/volumes_controller.rb:12-103` |
| `selfhost opensearch snapshots list` | `GET /aws/v1/snapshots` | Read-only. Every snapshot the AWS account owns in one region (account-wide, not org-scoped; `app/services/cloud_provider/aws/aws_backup_service.rb:439`) | `region` (defaults `us-east-1`), `cloud_credential_id` | `config/routes.rb:425`, `app/controllers/aws/v1/snapshots_controller.rb:12` |
| (no verb) | `GET /aws/v1/snapshots/:id` | Read-only. `describe_snapshot` for one id | path `id` | `config/routes.rb:425`, `app/controllers/aws/v1/snapshots_controller.rb:49` |
| `selfhost opensearch snapshots delete` | `DELETE /aws/v1/snapshots/:id` | **Destructive**. Needs a matching `Backup` row, which is ownership-checked | path `id` | `config/routes.rb:425`, `app/controllers/aws/v1/snapshots_controller.rb:103` |
| `selfhost opensearch backups list` | `GET /aws/v1/backups` | Read-only. Syncs, then lists the org's backup records with their AWS sizes | `organization_id` (injected) | `config/routes.rb:413`, `app/controllers/aws/v1/backups_controller.rb:22-39` |
| `selfhost opensearch backups create` | `POST /aws/v1/backups` | Mutating, **billable**. Body is `backup{…}`; for OpenSearch the only valid `backup_type` is `aws_backup_job` | `backup_type`, `resource_id` (instance pid), `organization_id`, `region`, `options{backup_vault_name,iam_role_arn,resource_arn,backup_plan_id}` | `config/routes.rb:413`, `app/controllers/aws/v1/backups_controller.rb:15,50-104,127-146` |
| `selfhost opensearch backups delete` | `DELETE /aws/v1/backups/:id` | **Destructive** (removes the snapshot/S3 objects too). Backup ids look like `back_<32 hex>` (`app/models/backup.rb:58`) | path `id` | `config/routes.rb:413`, `app/controllers/aws/v1/backups_controller.rb:538` |
| `selfhost opensearch backups restore` | `POST /aws/v1/backups/:id/restore` | **Refused for OpenSearch** — 422 `Only ClickHouse and Kafka native backups can be restored through this endpoint`. Restore via instance create + `snapshot_id` instead | — | `config/routes.rb:416`, `app/controllers/aws/v1/backups_controller.rb:289,306-310` |

### Backup policies (DLM)

No CLI verb yet; this is the DLM machinery `backup_enabled` uses at create. All
four are org-scoped; only create is balance-gated (`app/controllers/aws/v1/backup_policies_controller.rb:261`).
| Future CLI command | Method and path | What it does | Key parameters | Source |
|---|---|---|---|---|
| (no verb) | `GET /aws/v1/backup_policies` | Read-only. All policies, or one cluster's | `organization_id`, `cloud_instance_pid` | `config/routes.rb:427`, `app/controllers/aws/v1/backup_policies_controller.rb:19-33` |
| (no verb) | `POST /aws/v1/backup_policies` | Mutating. Flat top-level body (not nested) | `organization_id`, `region`, `policy_name`, `description`, `target_tags[]`, `schedule_interval` (int), `schedule_unit` (`HOURS`/`DAYS`/`WEEKS`), `retain_rule_count`, `starting_at[]`, `timezone`, `encryption`, `cloud_instance_pid`, `execution_role_arn` | `config/routes.rb:427`, `app/controllers/aws/v1/backup_policies_controller.rb:253-330,454-518` |
| (no verb) | `PATCH /aws/v1/backup_policies/:id` | Mutating. `state` pauses/resumes | `state` (`ENABLED`/`DISABLED`), `description`, `policy_details{policy_type,resource_types,target_tags,schedules}` | `config/routes.rb:427`, `app/controllers/aws/v1/backup_policies_controller.rb:145-233,947-961` |
| (no verb) | `DELETE /aws/v1/backup_policies/:id` | **Destructive**. `id` may be the DLM `external_policy_id` **or** the row pid | path `id`, `region` | `config/routes.rb:427`, `app/controllers/aws/v1/backup_policies_controller.rb:44,570-578` |

### Reference data

| Future CLI command | Method and path | What it does | Key parameters | Source |
|---|---|---|---|---|
| `selfhost catalog region list` | `GET /aws/v1/regions` | Read-only, unauthenticated | — | `config/routes.rb:435`, `app/controllers/aws/v1/regions_controller.rb:22` |
| `selfhost catalog instance-types` | `GET /aws/v1/regions/:region_code/instance_types` | Read-only, filterable | `family`, `current_gen`, `min_vcpu`, `min_memory` | `config/routes.rb:436`, `app/controllers/aws/v1/regions_controller.rb:34` |
| `selfhost catalog storage-types` | `GET /aws/v1/storage_types` | Read-only, optionally region-specific | `region` | `config/routes.rb:438`, `app/controllers/aws/v1/storage_types_controller.rb:15` |
| `selfhost catalog pricing` | `GET /aws/v1/pricing` | Read-only, unauthenticated — no organization needed, unlike most rows here. The monthly catalog: `currency`, `effective_region`, `supported_regions`, and each instance type in the region with `hourly_price` and `monthly_price`. A region the platform does not cover 422s | `region` (defaults `us-east-1`) | `config/routes.rb:444`, `app/controllers/aws/v1/pricing_controller.rb:6-20`, `app/services/pricing/hybrid_pricing_service.rb:143-176` |
| (no verb) | `POST /aws/v1/pricing/estimate` | Read-only quote, unauthenticated, flat body (not nested). Priced from the same rows the bill is built from, so a price or margin change moves the quote. Managed returns `unit_prices` plus a `breakdown` of the pre-margin base, the markup and the public-IPv4 line; `mode: byoc` returns a per-database rate instead. A bad region, instance type, storage type, size, IOPS or throughput 422s. `db_type` takes the engine name (`opensearch`) | `region`, `instance_type`, `storage_type`, `storage_size_gb`, `db_type`, `mode` (`managed`/`byoc`, default `managed`), `database_count`, `replica_count`, `public_ipv4_count`, `iops`, `throughput_mbps` | `config/routes.rb:445`, `app/controllers/aws/v1/pricing_controller.rb:22-49`, `app/services/pricing/hybrid_pricing_service.rb:22-140,205-255` |

## Provision one end to end

Reference data first: `region`, `type_of_dbms`, `identifier`, `instance_type`
and `storage` are required
(`app/validators/aws_validators/instances_validator.rb:46,92-137`).

```sh
selfhost api /aws/v1/regions -o json
selfhost api -X GET /aws/v1/regions/us-east-1/instance_types -f min_memory=4096
selfhost api -X GET /aws/v1/storage_types -f region=us-east-1
```

1. Create — **billable**. An org whose balance cannot cover the SKU gets 402
   (exit 4) before anything is provisioned (`app/controllers/aws/v1/instances_controller.rb:637-646`,
   `app/controllers/concerns/billing_gate.rb:11`). The body is nested, so it has
   to be raw JSON:

   ```sh
   cat > os.json <<'JSON'
   { "region": "us-east-1", "type_of_dbms": "opensearch", "identifier": "search-1",
     "instance_type": "r6g.large", "db_version": "3.8",
     "storage": { "type": "gp3", "size": 100, "iops": 3000, "throughput": 125 },
     "public_access": false, "delete_protection": false, "backup_enabled": true,
     "tags": [ { "key": "env", "value": "staging" } ] }
   JSON
   selfhost api /aws/v1/instances --input os.json -v
   ```

   Keep `pid`, `group_id` and `host` from `data`; do **not** add
   `organization_id` unless the target org differs from the resolved one.

2. Poll. The field that flips is `ready`, and the body is a one-element array:

   ```sh
   # read-only; repeat until data[0].ready is true
   selfhost api /aws/v1/instances/awsinst_0123456789abcdef -o json
   ```

   `ready` = `status == "running"` + `internal_status == "completed"` + no
   `error_message` + healthy agent, plus `ha_ready` across the group
   (`app/models/cloud_instance.rb:1853-1880`). While false, `task_progress` lists the agent
   tasks; a failed install sets `error_message` and puts `has_error` in
   `readiness_detail.reasons`, and `readiness_detail.stuck` flags a timeout
   (`app/models/cloud_instance.rb:1932-1985`). Stop polling on either.

3. Connection details are on that same response: `dns_name`, `effective_port`
   (9200), `username` (default `appuser`) and `init_pg_pass` — the customer
   password, returned on this authenticated read and on the org-wide list, which uses
   the same serializer
   (`app/controllers/aws/v1/instances_controller.rb:1153-1168,1326`, `app/serializers/cloud_instance_serializer.rb:45`).
   Only create and fork deliberately omit it.
   `shdev_admin_pass` (the engine `admin` superuser) is never exposed.

4. Optionally enable Dashboards and read the URL back:

   ```sh
   selfhost api -X PATCH /aws/v1/instances/awsinst_0123456789abcdef -F dashboards_enabled=true
   selfhost api /aws/v1/instances/awsinst_0123456789abcdef/dashboards
   ```

5. Tear down — **destructive**. The request must carry the master and every
   replica, and delete protection must be off. A multi-pid batch needs a JSON array,
   so flat `-f` keys cannot express it — a single pid 422s on an HA group
   (`app/controllers/aws/v1/instances_controller.rb:1398-1408`):

   ```sh
   selfhost api -X PATCH /aws/v1/instances/awsinst_0123456789abcdef -F delete_protection=false
   printf '{"pids":["awsinst_MASTER","awsinst_REPLICA"]}' > pids.json
   selfhost api -X POST /aws/v1/instances/delete --input pids.json
   ```

## Day-2 operations

| Operation | How |
|---|---|
| start / stop / reboot | `PUT /aws/v1/instances/{start,stop,reboot}` with the master's pid in `pids`; the group follows. Stop requires `running`, start requires `stopped` (`app/controllers/aws/v1/instances_controller.rb:1832-1836`, `app/models/cloud_instance.rb:810-811`) |
| resize | `PUT /aws/v1/instances/:pid/scale`. Storage always; instance type through the `:provision_switchover` gate, which OpenSearch passes because `supports_zero_data_loss_promotion?` is `true` — the inline comment there claims the opposite (`app/services/database_adapters/opensearch_adapter.rb:88-90`, `app/controllers/aws/v1/instances_controller.rb:3166-3198`). The new size must exceed the current one (`app/services/instance_scaling_validator_service.rb:47-52`) |
| fork | `POST /aws/v1/instances/:pid/fork` with `instance_name`. Always a single-node child; the reply has no pid, so find it by `fork_source_instance_pid` (`app/controllers/aws/v1/instances_controller.rb:3119-3133`) |
| failover | `POST /aws/v1/instances/:pid/trigger_failover`. Paid org, `multi_az`, master, a running replica, no pending task, plus a cooldown (`app/controllers/aws/v1/instances_controller.rb:3411-3500`) |
| backup / snapshot | On demand `POST /aws/v1/volumes/create_snapshot`; for a schedule set `backup_enabled: true` or create a DLM policy |
| restore | Only `POST /aws/v1/instances` with `snapshot_id` — the originating record wins first (its engine and version overwrite any override; `app/controllers/aws/v1/instances_controller.rb:511-513`); snapshot tags are only the fallback when no record resolves (`app/controllers/aws/v1/instances_controller.rb:583-588`) |
| logs / stats | POST to dispatch, then GET the same path for the fetch row. Paid plan, running instance, agent |

## Gotchas

- **An organization must resolve**, or `GET /aws/v1/backups` and every
  `/aws/v1/snapshots` call 404 as `Organization not found` — both read the
  injected `organization_id` (`app/controllers/aws/v1/backups_controller.rb:879-884`,
  `app/controllers/aws/v1/snapshots_controller.rb:159-165`).
- **No server-side engine filter on `list`**: `by_organization` returns every
  group in the org and ignores `limit`; drop non-OpenSearch rows yourself.
- **Create needs raw JSON** — `storage`, `tags`, `dlm_policy_config` and
  `allowed_cidr_ranges` are nested, so flat `-f` keys 422. **Create takes no
  `database_user`** — it is silently dropped; the only user it makes is the
  bootstrap admin on `running` (`app/models/cloud_instance.rb:1568-1577`).
- **HA is a topology, not a toggle.** `multi_az: true` requires
  `replica_count >= 1` at create (`app/controllers/aws/v1/instances_controller.rb:377-383`), which
  yields 3 dedicated cluster-manager nodes plus `replica_count + 1` data nodes
  (`app/services/opensearch/topology.rb:14,46-47`). Below the dedicated-manager threshold there
  are no manager nodes at all: the all-in-one nodes run on the data instance type
  (`app/services/opensearch/topology.rb:99-106`); `t4g.medium` is only the default for a blank
  manager type on the dedicated path (`app/services/opensearch/topology.rb:35,67`), so nothing
  is substituted. Flipping `multi_az` later is refused outright
  (`app/controllers/aws/v1/instances_controller.rb:2400-2420`).
- **Dashboards has no enable/disable route**: `PATCH /aws/v1/instances/:pid` with
  `dashboards_enabled` is the whole write API.
- **Security-plugin built-ins are reserved** — `admin`, `kibanaserver`,
  `kibanaro`, `logstash`, `readall`, `snapshotrestore`. Instance create rejects such a
  `username` up front (`app/controllers/aws/v1/instances_controller.rb:842-849`). The user
  create endpoint does not: it validates only required fields, status and conflicts
  (`app/controllers/aws/v1/database_users_controller.rb:35-86`), so a reserved name passes
  creation and fails later on-host; only update and password rotation refuse a reserved row
  (`app/controllers/aws/v1/database_users_controller.rb:103,149`). Deleting a non-bootstrap
  reserved row succeeds by removing the platform row (`app/controllers/aws/v1/database_users_controller.rb:195-199`);
  `rotate-password` returns its cleartext once (`app/controllers/aws/v1/database_users_controller.rb:157-162`).
- **PITR is unsupported** (`app/services/database_adapters/opensearch_adapter.rb:50-52`): `pitr_enabled` is
  forced `false` at create and the model rejects it afterwards.
- **A per-user connection limit cannot be set on OpenSearch.** The user
  create and update endpoints both accept the field, but only Postgres and
  MySQL apply it, so any non-null value is refused with `Connection limit is
  not supported for opensearch instances` and nothing is written
  (`app/models/database_user.rb:46,177-183`). Pass it as an explicit `null` to
  clear a value that is already stored.
- **Restore is not `POST /aws/v1/backups/:id/restore`** — that 422s here. Use
  instance creation with `snapshot_id`.
- **`POST /aws/v1/backups` needs `options.backup_vault_name`** and a resolvable
  AWS Backup IAM role, or it 422s before any record exists
  (`app/controllers/aws/v1/backups_controller.rb:15,127-146`).
- **Delete is a pid batch**: a master cannot go unless every replica in its
  group is in the same `pids` list (`app/controllers/aws/v1/instances_controller.rb:1398-1408`). With
  `create_snapshot=true` the snapshots are EBS — right for OpenSearch, whose data
  lives on the volume (`app/services/database_adapters/base.rb:107-113`).
- **Rate limits and cooldowns**: create 10/min per user, other mutations 10/min
  per user+IP, delete 5/min, fork 3/min (`app/controllers/aws/v1/instances_controller.rb:92-105`); a 429
  sleeps once then exits 75. Log fetches are one per 10 s per source, stats
  fetches one per 15 s (`app/controllers/aws/v1/logs_controller.rb:6`, `app/controllers/aws/v1/db_stats_controller.rb:10`).

## Sources

- `config/routes.rb:326-346,348-366,413-427,435-445`.
- `app/controllers/aws/v1/instances_controller.rb` (`create` `108`, `details`
  `1225`, `destroy` `1346`, `stop` `1822`, `update` `2045`, `fork` `2845`,
  `scale` `3151`, `dashboards` `3379`, `trigger_failover` `3411`) and the sibling
  `aws/v1/{database_users,logs,db_stats,volumes,snapshots,backups,backup_policies,regions,storage_types,pricing}_controller.rb`.
- `app/models/cloud_instance.rb`, `app/services/opensearch/{topology,promotion_service}.rb`,
  `app/services/database_adapters/opensearch_adapter.rb`, `app/services/instance_scaling_validator_service.rb`,
  `app/validators/aws_validators/instances_validator.rb`, `app/jobs/update_instance_job.rb`,
  `app/serializers/{cloud_instance,database_user}_serializer.rb`.
- `app/models/database_user.rb`, `app/services/pricing/hybrid_pricing_service.rb`.
