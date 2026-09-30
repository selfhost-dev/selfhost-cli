# Scaling

Cluster capacity work: scaling policies, the capacity-ladder reference data,
per-group capacity configs, and the cluster scale plans that execute tier moves.
Every `selfhost scaling ...` verb — including `scaling capacity ladder|config|plan`
— answers `not implemented yet: scaling <verb>` today, so the work goes through
`selfhost api` with the paths below. Read [api.md](api.md) first for method
defaulting, `-f` vs `-F`, `--input` and exit codes.

Two request/response shapes live here; do not mix them up:

| Family | Prefix | Request body | Response |
| --- | --- | --- | --- |
| Scaling policies | `/scaling_policies` (top level, no `/api/v1`) | nested `scaling_policy` object | JSON:API — the wire body is `{data: …}`, so the printed payload is the resource: `{id, type, attributes}` (an array for `list`) |
| Ladder, capacity config, scale plans | `/api/v1/...` | flat parameters | `{status, data, message, status_code}` envelope |

All of it is org-scoped: `organization_id` (the CLI injects it on every call) or
`organization_pid` must resolve, otherwise the call 404s
(`app/controllers/application_controller.rb:112`, `:168`).

Pids: policy `scaling_policy_<hex>` (`app/models/scaling_policy.rb:125`), capacity
config `gcc_<ulid>` (`app/models/group_capacity_config.rb:62`), scale plan
`csp_<ulid>` (`app/models/cluster_scale_plan.rb:541`).

`group_id` is not a resource of its own: it is the `grp_…` cluster key every
`CloudInstance` carries (`app/models/cloud_instance.rb:2170`,
`app/serializers/cloud_instance_serializer.rb:43`); one instance-create request mints
one id, so a master and its replicas share it
(`app/controllers/aws/v1/instances_controller.rb:183`). Take it from the listing.

## Before you start

```sh
selfhost auth status                 # exit 3 when signed out
selfhost org use acme                # or pass --org acme / --org org_…
```

- `api` never prompts: `-y`/`--poll-interval` do nothing, `--dry-run` exits 2.
- A GET that carries parameters needs `-X GET`, or the `-f`/`-F` turn it into a POST.
- `-f` sends strings, `-F` types `true`/`false`/`null`/integers, `--input -` sends a
  raw body verbatim; the CLI adds no path prefix.
- `{org}` is only a CLI placeholder for the org pid; these paths use `group_id`
  and pids instead, so it never appears below.
- Everything mutating here changes or destroys cluster state; nothing is billable
  by itself, but a resize changes instance cost.

## Endpoint map

### Scaling policies — JSON:API, nested body

| Future CLI command | Method and path | What it does | Key parameters | Source |
| --- | --- | --- | --- | --- |
| `selfhost scaling list` | `GET /scaling_policies` | read-only; policies of the resolved org | – | `config/routes.rb:461`, `app/controllers/scaling_policies_controller.rb:13` |
| `selfhost scaling show` | `GET /scaling_policies/:pid` | read-only; one policy | – | `config/routes.rb:461`, `app/controllers/scaling_policies_controller.rb:19` |
| `selfhost scaling create` | `POST /scaling_policies` | mutating; body must be `{"scaling_policy":{…}}` | `organization_pid`, `scaling_action`, `group_id`, `target_class`, `direction`, `window_minutes`, `cpu_up_threshold`, `memory_up_threshold`, `cpu_down_threshold`, `memory_down_threshold` | `config/routes.rb:461`, `app/controllers/scaling_policies_controller.rb:24`, `:223` |
| `selfhost scaling update` | `PUT /scaling_policies/:pid` (`PATCH` also matches) | mutating; same nested body | any permitted policy field | `config/routes.rb:461`, `app/controllers/scaling_policies_controller.rb:123` |
| `selfhost scaling delete` | `DELETE /scaling_policies/:pid` | destructive | – | `config/routes.rb:461`, `app/controllers/scaling_policies_controller.rb:134` |

Policy fields are the serializer's attribute list
(`app/serializers/scaling_policy_serializer.rb:4-12`) plus the computed
`cloud_instance_pids` (`:14-16`). Tier policies (`resize_instance` + `group_id`,
`app/models/scaling_policy.rb:64`) are the ones this domain uses: they validate
`direction`, `target_class` and `window_minutes` (`:46-48`), require
`cloud_instance_pid` to be absent (`:53`), and require an enabled
`GroupCapacityConfig` for `(group_id, target_class)` to exist (`:169-175`).

### Capacity ladder — reference data

| Future CLI command | Method and path | What it does | Key parameters | Source |
| --- | --- | --- | --- | --- |
| `selfhost scaling capacity ladder` | `GET /api/v1/capacity_ladders` | read-only; ordered ladder per profile | `db_type` (required), `cloud_provider` (default `aws`) | `config/routes.rb:177`, `app/controllers/api/v1/capacity_ladders_controller.rb:8` |

The printed payload is `{db_type, cloud_provider, profiles: {<profile>: [{index, instance_type, vcpu, memory_gib}, …]}}`
(`app/controllers/api/v1/capacity_ladders_controller.rb:21-33`). `index` is the
tier index used by the capacity config; `vcpu`/`memory_gib` come from the
provider spec files (`app/services/capacity_ladder_loader.rb:114-121`). The YAML
is the source of truth: `config/data/capacity_ladders.yml`.

### Per-group capacity config

| Future CLI command | Method and path | What it does | Key parameters | Source |
| --- | --- | --- | --- | --- |
| `selfhost scaling capacity config` | `GET /api/v1/groups/:group_id/capacity_config` | read-only; config + live tier state | `target_class` (default `data`; other value `non_data`) | `config/routes.rb:179`, `app/controllers/api/v1/group_capacity_configs_controller.rb:15` |
| `selfhost scaling capacity config` (set) | `POST /api/v1/groups/:group_id/capacity_config` | mutating; flat body; requires an org member instance | `target_class`, `profile`, `min_tier_index`, `max_tier_index`, `deprecation_window_minutes`, `enabled` | `config/routes.rb:180`, `app/controllers/api/v1/group_capacity_configs_controller.rb:24`, `:73` |
| `selfhost scaling capacity config` (change) | `PUT /api/v1/groups/:group_id/capacity_config/:pid` | mutating; `PUT` only (`PATCH` 404s) | same fields; omitting `target_class` leaves `profile` untouched | `config/routes.rb:181`, `app/controllers/api/v1/group_capacity_configs_controller.rb:35` |
| `selfhost scaling capacity config` (clear) | `DELETE /api/v1/groups/:group_id/capacity_config/:pid` | destructive; 422 while an enabled `resize_instance` policy references it | – | `config/routes.rb:182`, `app/controllers/api/v1/group_capacity_configs_controller.rb:48` |

Show/create/update print `{config: {pid, group_id, organization_pid,
target_class, profile, min_tier_index, max_tier_index, deprecation_window_minutes,
enabled, created_at, updated_at}, current_tier_index, current_instance_type,
resolved_ladder, instances: [{pid, name, role, instance_type, status,
current_capacity_tier_index}]}` (`app/controllers/api/v1/group_capacity_configs_controller.rb:96-128`).
For `target_class=data` the same payload also carries `active_scale_plan`,
`desired_tier_index`, `last_failed_scale_plan`, and — single-node in-place moves
have no plan — `active_tier_event` / `last_tier_event` (`:129-172`).

### Cluster scale plans

Plans have no list/show endpoint: read them out of the `data`-class config
payload (`active_scale_plan`, `last_failed_scale_plan`); their `pid` is the
`csp_…` used below.

| Future CLI command | Method and path | What it does | Key parameters | Source |
| --- | --- | --- | --- | --- |
| `selfhost scaling capacity plan` (retry) | `POST /api/v1/cluster_scale_plans/:pid/retry` | mutating; only a `failed` plan, re-enqueues the executor | – | `config/routes.rb:184`, `app/controllers/api/v1/cluster_scale_plans_controller.rb:27` |
| `selfhost scaling capacity plan` (abandon) | `DELETE /api/v1/cluster_scale_plans/:pid` | destructive; only a `failed` plan; audit-logged | – | `config/routes.rb:185`, `app/controllers/api/v1/cluster_scale_plans_controller.rb:76` |

Retry prints the plan object:
`{pid, group_id, cloud_instance_pid, desired_tier_index_before, desired_tier_index_after,
target_instance_type, status, ordered_node_replacements, started_at, completed_at,
error_message}` (`app/controllers/api/v1/cluster_scale_plans_controller.rb:118-133`); destroy replies with no `data`, so it prints `null` — the exit code is the signal.

## Read the ladder, set a config, retry a failed plan

```sh
# 1. read-only: the org's instances, already grouped by group_id — take the
#    group_id of the ClickHouse cluster you are sizing
selfhost api /aws/v1/instances/by_organization -o json

# 2. read-only: the ladder for that cluster (AWS is the executed provider in v1)
selfhost api -X GET /api/v1/capacity_ladders -f db_type=clickhouse -f cloud_provider=aws -o json

# 3. read-only: does this group already have a data config, and where is it now?
selfhost api -X GET /api/v1/groups/grp_1a2b3c4d5e/capacity_config -f target_class=data -o json
```

Pick `min_tier_index` / `max_tier_index` from the ladder `index` values in step 2:
they are positions in `profiles.default`, not instance types. Then create the
config (a group may hold one `data` and one `non_data` config):

```sh
# 4. mutating: bounds + profile for the data plane of the group
#    -F types the indices, so they arrive as JSON numbers
selfhost api /api/v1/groups/grp_1a2b3c4d5e/capacity_config \
  -F target_class=data -f profile=default \
  -F min_tier_index=0 -F max_tier_index=6 -F enabled=true -o json
```

The reply carries `config.pid` (keep it for updates/deletes),
`current_tier_index`, `current_instance_type`, the `resolved_ladder` it will move
through, and the `instances` in scope. Creating the config also writes
`current_capacity_tier_index`: for a `data` config only on the anchor instance
(master for multi-node, sole instance for single-node); every governed instance
is stamped only for `non_data` configs
(`app/models/group_capacity_config.rb:71-81`).

Tier policies are what actually trigger a move; they are rejected until the
config exists (`app/models/scaling_policy.rb:169-175`). Their body is nested, so
`-f` cannot build it — send JSON on stdin:

```sh
# 5. mutating: scale up when the group is hot. For a group-scoped policy
#    organization_pid is optional: create fills it in from the group anchor
#    when omitted (omitting it also skips the paid-plan check); --input keeps
#    the body exactly as sent.
echo '{"scaling_policy":{"organization_pid":"org_1a2b3c4d5e","group_id":"grp_1a2b3c4d5e","target_class":"data","direction":"scale_up","scaling_action":"resize_instance","window_minutes":10,"cpu_up_threshold":80,"memory_up_threshold":80,"enabled":true}}' \
  | selfhost api /scaling_policies --input - -o json

# 6. read-only: poll the config payload until the move shows up.
#    HA data: active_scale_plan leaves the payload, then either
#    last_failed_scale_plan or the anchor tier index changes.
#    Single-node: active_tier_event / last_tier_event instead.
selfhost api -X GET /api/v1/groups/grp_1a2b3c4d5e/capacity_config -f target_class=data -o json

# 7. mutating: a failed plan — retry it. Plan-owned repair is engine-specific
#    and resume-based; completed node replacements stay completed.
selfhost api -X POST /api/v1/cluster_scale_plans/csp_01h2x3y4z5/retry -o json

# 8. destructive, alternative to 7: abandon the failed plan so it stops
#    blocking new tier decisions for the group
selfhost api -X DELETE /api/v1/cluster_scale_plans/csp_01h2x3y4z5
```

Retry semantics: only `status == "failed"` is retried
(`app/controllers/api/v1/cluster_scale_plans_controller.rb:33-35`); a plan-owned
mutation task still `in_progress` on an agent refuses the retry with 422 to avoid
double execution (`:42-48`); otherwise state is repaired, status flips to
`in_progress`, `error_message` clears, and the executor is enqueued — ClickHouse via
`prepare_for_retry!` + `ClusterScalePlanExecutor`, Kafka via `prepare_kafka_retry!` +
`KafkaScalePlanExecutor` with the trigger event reopened (`:58-66`). Destroy deletes
a failed plan (audit-logged); an already-deleted pid is a clean 404 (`:76-90`).

## Day-2 operations

| Operation | Call |
| --- | --- |
| Change bounds, deprecation window or ladder `profile` | `selfhost api -X PUT /api/v1/groups/:group_id/capacity_config/:pid -F max_tier_index=8`; a new `profile` must exist for the anchor's engine/provider and keep the current tier index inside the bounds (`app/models/group_capacity_config.rb:99-104`, `:130-140`) |
| Pause tier decisions for a group | `selfhost api -X PUT /api/v1/groups/:group_id/capacity_config/:pid -F enabled=false` (the decision loop skips disabled configs, `app/services/autoscaling_decision_service.rb:354-355`) |
| Retry / abandon a failed plan | `selfhost api -X POST /api/v1/cluster_scale_plans/:pid/retry`, `selfhost api -X DELETE /api/v1/cluster_scale_plans/:pid` |
| Delete a config | `selfhost api -X DELETE /api/v1/groups/:group_id/capacity_config/:pid` — 422 while an enabled `resize_instance` policy references the pair |

## Gotchas

- **`group_id` has no endpoint of its own.** It comes from a `CloudInstance`
  (`grp_…`); the config endpoints 404 with `No instances found for group … in this
  organization` when no live instance of your org carries it
  (`app/controllers/api/v1/group_capacity_configs_controller.rb:66-70`). The
  `group_id` field (`app/serializers/cloud_instance_serializer.rb:43`) and the
  `GET /aws/v1/instances/by_organization` grouping
  (`app/controllers/aws/v1/instances_controller.rb:1321`) are the two reliable
  sources; see the engine references (for example clickhouse.md) for the instance side.
- **Bounds are ladder indices, not types or resources.** `min_tier_index` /
  `max_tier_index` must be `0..entries.length-1` of the resolved profile
  (`app/models/group_capacity_config.rb:106-116`), `min ≤ max` (`:83-88`), and an
  update is rejected when the anchor's current index falls outside the new bounds
  (`:130-140`). Configs are ClickHouse-only in v1 (`:91-96`).
- **`profile` defaulting is one-way.** `data` → `default`, `non_data` →
  `auxiliary`, but only when the request carries `target_class`; an update that
  omits it (a bounds-only tweak) does not rewrite `profile`
  (`app/controllers/api/v1/group_capacity_configs_controller.rb:85-90`). `PATCH`
  on the config path is a 404 — the route is `put` (`config/routes.rb:181`).
- **A config change restarts nothing by itself.** It only changes the bounds the
  next decision reads; the decision loop runs on `AutoscalingMonitorJob` every
  minute (`config/recurring.yml:33-36`,
  `app/services/autoscaling_decision_service.rb:14`). Node bounces come from
  execution: single-node ClickHouse data and keepers resize in place —
  stop → modify instance type → start (`app/jobs/instance_scaling/in_place_resize_job.rb:4-13`)
  — while HA data nodes are replaced one at a time by a scale plan, the replaced
  node being terminated only after `deprecation_window_minutes` (default `120`,
  `app/models/group_capacity_config.rb:14`, `:66`).
- **One active plan per group.** A unique index allows one `pending`/`in_progress`
  plan per `group_id` (`db/schema.rb:699`); a `failed` plan blocks *all* new tier
  decisions for the group until it is retried, deleted, or its 6 h grace window
  lapses (`app/models/cluster_scale_plan.rb:25-28`, `:56-59`,
  `app/services/autoscaling_decision_service.rb:363-369`).
- **Retry can 422 twice over.** Corrupt plan state (a persisted chain pid that no
  longer resolves to a task) and an ambiguous Kafka leg both answer 422 with the
  message, and in the Kafka case nothing is enqueued
  (`app/controllers/api/v1/cluster_scale_plans_controller.rb:16-24`). Only
  `failed` plans can be retried or deleted (`:33`, `:83`).
- **Policy calls need a paid org and the nested body.** `create`/`update`/`delete`
  run `require_paid_plan!` and 422 with `Top up your balance to use this feature.`
  when the org has no paid plan (`app/controllers/scaling_policies_controller.rb:205-212`);
  for group-scoped tier policies the org id is optional in the body: create fills
  it in from the group anchor when omitted (leaving it out also skips the
  paid-plan check), and update/delete read the saved policy's org — `--input -`
  (or a JSON file) is the only way to send the nested body, since `-f`
  cannot nest. `enabled` has no DB default (`db/schema.rb:1728`) and must be sent,
  or inclusion fails (`app/models/scaling_policy.rb:31`).
- **The two families answer differently.** `/scaling_policies` answers with a
  JSON:API document whose own `data` key the CLI strips, so the printed payload is
  the resource (`attributes`, an array on `list`) and there is no `message`; the
  `/api/v1/...` endpoints return the `{status, data, message}` envelope. Either
  way `api` prints only the envelope-level `data` — `-i` adds the `HTTP <status>`
  line and headers, never the wrapper. `list` is org-filtered by
  `organization_pid` (`app/controllers/scaling_policies_controller.rb:14`).
- **The ladder is not the same as availability.** It lists the types a move may
  pick; when a tier is chosen, candidates unavailable in the node's region are
  skipped (`app/services/capacity_tier_decision_service.rb:160-170`, `:177-183`).
  Tier execution is AWS-only in v1 — Hetzner ladders are config-only
  (`config/data/capacity_ladders.yml:11-13`) — and `db_type` must be a
  `DatabaseAdapters::Registry.supported_types` value or the call 422s
  (`app/controllers/api/v1/capacity_ladders_controller.rb:12-14`).

## Sources

- `config/routes.rb:177-185` (ladder, capacity config, scale plans), `:330`
  (instance listing), `:461` (`resources :scaling_policies`); `config/recurring.yml:33-36`; `db/schema.rb:699`.
- Controllers: `app/controllers/scaling_policies_controller.rb`,
  `app/controllers/api/v1/{capacity_ladders,group_capacity_configs,cluster_scale_plans}_controller.rb`,
  `app/controllers/application_controller.rb`, `app/controllers/concerns/response_handler.rb`,
  `app/controllers/aws/v1/instances_controller.rb`.
- Models: `app/models/{scaling_policy,group_capacity_config,cluster_scale_plan,cloud_instance}.rb`.
- Services/jobs: `app/services/{capacity_ladder_loader,cluster_anchor,capacity_tier_decision_service,autoscaling_decision_service}.rb`,
  `app/jobs/autoscaling_monitor_job.rb` and `app/jobs/instance_scaling/{in_place_resize_job,cluster_scale_plan_executor}.rb`.
- Data and serializers: `config/data/capacity_ladders.yml`, `config/data/aws_instance_specs.json`, `app/serializers/{scaling_policy,cloud_instance}_serializer.rb`.
