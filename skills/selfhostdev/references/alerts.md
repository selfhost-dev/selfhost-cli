# Alerts

Three resources behind `selfhost alert …`, all still `not implemented yet`:
**alert rules** (what to watch and when to fire, pid `alrt_…`), **alert
instances** (one row per rule × instance that fired, pid `alinst_…`) and
**notification channels** (where a notification is delivered, pid `nchan_…`).
All three are nested under the organization
(`/aws/v1/organizations/:organization_id/…`); scoping uses `@current_organization`,
resolved from `params[:organization_id]` (the `{org}` segment) plus a membership check.

Instances are **not created by you**: the evaluator creates one per rule/instance
pair on that pair's first evaluation — before anything fires, and even when the
heartbeat is missing or stale (`state_manager.rb:112-121`) — then moves it through
`normal → pending → alerting → resolved`. The API only reads them and applies two
manual transitions, `acknowledge` and `resolve`. A rule with no channel binding
notifies nobody (`app/services/alerting/notification_service.rb:15-18`).

## Before you start

- `selfhost auth status` — a signed-in profile and a resolved organization
  (`--org <slug>` / `selfhost org use <slug>`). `{org}` expands to the org **pid**;
  `@current_organization` comes from `params[:organization_id]`
  (`app/controllers/application_controller.rb:123`), which the path segment supplies.
- Read `references/api.md`. Here: a GET with parameters needs `-X GET` or `-f`
  turns it into a POST; `-f` sends raw strings, `-F` types them; and **rule and
  channel create/update bodies are nested**, which `-f key[sub]` cannot build —
  those calls must use `--input -` with `{"alert_rule":{…}}` /
  `{"notification_channel":{…}}`. Instance resolve takes a flat top-level
  `reason` (`-f reason=…`, step 6 below) and acknowledge/test take no body.
- Permissions by role (`db/seeds.rb:27-29`, `:53-55`, `:69-70`): Admin all; Manager all
  but alert-rule `destroy`; Member: rules read, instances read+acknowledge, no channels.

## Endpoint map

### Alert rules

| Future CLI command | Method and path | What it does | Key parameters | Source |
|---|---|---|---|---|
| `selfhost alert rules list` | `GET /aws/v1/organizations/{org}/alert_rules` | List rules, newest first | `cloud_instance_pid` (optional filter) | `config/routes.rb:394`, `app/controllers/aws/v1/alert_rules_controller.rb:17-19` |
| `selfhost alert rules show` | `GET /aws/v1/organizations/{org}/alert_rules/:id` | One rule by pid | path `id` (`alrt_…`) | `config/routes.rb:394`, `alert_rules_controller.rb:28` |
| `selfhost alert rules create` | `POST /aws/v1/organizations/{org}/alert_rules` | Create a rule | nested `alert_rule` object; see fields below | `config/routes.rb:394`, `alert_rules_controller.rb:33`, `:147` |
| `selfhost alert rules update` | `PATCH /aws/v1/organizations/{org}/alert_rules/:id` | Change a rule | nested `alert_rule` object | `config/routes.rb:394`, `alert_rules_controller.rb:70` |
| `selfhost alert rules delete` | `DELETE /aws/v1/organizations/{org}/alert_rules/:id` | Delete a rule (instances cascade) | path `id` | `config/routes.rb:394`, `alert_rules_controller.rb:119` |

Rule fields the action permits (`alert_rules_controller.rb:147-155`):
`name`, `description`, `severity`, `metric_type`, `metric_name`, `operator`,
`threshold`, `mount_point`, `evaluation_interval_seconds`, `for_duration_seconds`,
`notification_cooldown_seconds`, `enabled`, `cloud_instance_pid`.
`notification_channel_pids` is **not** permitted but is read straight off the same
nested object (`alert_rules_controller.rb:193`) — an array of channel pids that
replaces the rule's bindings; on update it is only synced when the key is present
(`:81`). A channel pid not owned by the org is dropped silently (`:196-199`).

Server defaults (schema): `enabled true`, `severity "warning"`,
`evaluation_interval_seconds 60`, `for_duration_seconds 0`,
`notification_cooldown_seconds 300`. `enabled_channels` defaults to `["email"]` but
is not settable and routes nothing — bindings do.

Response shape (`app/serializers/alert_rule_serializer.rb`): `pid`, `name`,
`description`, `severity`, `metric_type`, `metric_name`, `operator`,
`threshold`, `mount_point`, `evaluation_interval_seconds`,
`for_duration_seconds`, `notification_cooldown_seconds`, `enabled`,
`cloud_instance_pid`, `created_at`, `updated_at`, `notification_channel_pids`.

### Alert instances (read-mostly)

| Future CLI command | Method and path | What it does | Key parameters | Source |
|---|---|---|---|---|
| `selfhost alert instances list` | `GET /aws/v1/organizations/{org}/alert_instances` | List instances, `updated_at` desc | `state`, `cloud_instance_pid`, `alert_rule_pid`, `page`, `items` | `config/routes.rb:395`, `alert_instances_controller.rb:13-29` |
| `selfhost alert instances show` | `GET /aws/v1/organizations/{org}/alert_instances/:id` | One instance by pid | path `id` (`alinst_…`) | `config/routes.rb:395`, `alert_instances_controller.rb:46` |
| `selfhost alert instances ack` | `POST /aws/v1/organizations/{org}/alert_instances/:id/acknowledge` | Stamp `acknowledged_at` / `acknowledged_by_user_pid` | path `id` | `config/routes.rb:397`, `alert_instances_controller.rb:51` |
| `selfhost alert instances resolve` | `POST /aws/v1/organizations/{org}/alert_instances/:id/resolve` | `state` → `resolved` | `reason` (optional) | `config/routes.rb:398`, `alert_instances_controller.rb:85` |

`state` filter takes `normal|pending|alerting|resolved`
(`app/models/alert_instance.rb:4`). `items` defaults to 25, clamped to 100;
`page` starts at 1. This index alone wraps a `pagination` object beside `data`
(`alert_instances_controller.rb:27-43`).

Instance response fields (`app/serializers/alert_instance_serializer.rb`):
`pid`, `alert_rule_pid`, `cloud_instance_pid`, `organization_pid`, `state`,
`previous_state`, `state_reason`, `current_value`, `threshold_value`,
`condition_met_at`, `fired_at`, `resolved_at`, `last_notified_at`,
`acknowledged_at`, `acknowledged_by_user_pid`, `resolved_by_user_pid`,
`created_at`, `updated_at`.

### Notification channels

| Future CLI command | Method and path | What it does | Key parameters | Source |
|---|---|---|---|---|
| `selfhost alert channels list` | `GET /aws/v1/organizations/{org}/notification_channels` | List channels, newest first | — (no pagination) | `config/routes.rb:391`, `notification_channels_controller.rb:25` |
| `selfhost alert channels show` | `GET /aws/v1/organizations/{org}/notification_channels/:id` | One channel by pid | path `id` (`nchan_…`) | `config/routes.rb:391`, `notification_channels_controller.rb:35` |
| `selfhost alert channels create` | `POST /aws/v1/organizations/{org}/notification_channels` | Create a channel | nested `notification_channel`; `name`, `channel_type`, `enabled`, `channel_config` | `config/routes.rb:391`, `notification_channels_controller.rb:43`, `:90` |
| `selfhost alert channels update` | `PATCH /aws/v1/organizations/{org}/notification_channels/:id` | Rename, enable/disable, rewrite config | nested `notification_channel`; `name`, `enabled`, `channel_config` | `config/routes.rb:391`, `notification_channels_controller.rb:59`, `:94` |
| `selfhost alert channels delete` | `DELETE /aws/v1/organizations/{org}/notification_channels/:id` | Delete a channel (bindings cascade) | path `id` | `config/routes.rb:391`, `notification_channels_controller.rb:71` |
| `selfhost alert channels test` | `POST /aws/v1/organizations/{org}/notification_channels/:id/test` | Queue one test email | path `id` | `config/routes.rb:392`, `notification_channels_controller.rb:77` |

`channel_type` is **immutable after creation** — the update action does not permit
it (`notification_channels_controller.rb:94`). `show` is authorized with the
`index` permission, not a `show` one (`:10`).

Channel response fields (`app/serializers/notification_channel_serializer.rb`):
`pid`, `name`, `channel_type`, `channel_config`, `enabled`, `verified`,
`created_by_user_pid`, `organization_pid`, `created_at`, `updated_at`.

## Provision one end to end

1. **Create a channel** (mutating; rate-limited to 10/minute per user+IP,
   `notification_channels_controller.rb:16-18`). Only `email` exists and
   `channel_config` must carry a valid one (`app/models/notification_channel.rb:6`,
   `:44-48`); the body is nested, so it goes through `--input -`:

   ```sh
   selfhost api /aws/v1/organizations/{org}/notification_channels --org acme --input - <<'JSON'
   {"notification_channel":{"name":"On-call email","channel_type":"email","enabled":true,
    "channel_config":{"email":"oncall@example.com"}}}
   JSON
   ```

   Keep `data.pid` (`nchan_…`); `channel_config` is echoed back in full, so treat
   the response as sensitive.

2. **Test it** (mutating; rate-limited to 3/minute, `:20-22`). It ignores the
   channel kind and always queues `AlertMailer.test_email` to
   `channel_config["email"]` (`notification_channels_controller.rb:78`,
   `app/mailers/alert_mailer.rb:60`) — a success means "queued", nothing more.

   ```sh
   selfhost api -X POST /aws/v1/organizations/{org}/notification_channels/nchan_…/test --org acme
   ```

3. **Create a rule on a real metric** (mutating). `metric_type`/`metric_name`
   must be a valid pair or the save is 422 (`app/models/alert_rule.rb:56`,
   `:111-118`); cross-cutting types are `system`, `network` and `storage`
   (`alert_rule.rb:4-16`), the database ones are in Gotchas. Bind the channel with
   `notification_channel_pids` inside the same `alert_rule` object:

   ```sh
   selfhost api /aws/v1/organizations/{org}/alert_rules --org acme --input - <<'JSON'
   {"alert_rule":{"name":"High CPU","metric_type":"system","metric_name":"cpu_percent",
    "operator":">","threshold":80,"severity":"warning","for_duration_seconds":300,
    "notification_channel_pids":["nchan_…"]}}
   JSON
   ```

   `data` is the rule with `pid` `alrt_…`. Scope it to one database with
   `"cloud_instance_pid":"awsinst_…"`; omit it for an org-wide rule that fans out over
   every master instance with status `running` or `configuring_multi_az` and a healthy agent
   (`app/services/alerting/evaluation_service.rb:39-53`).

4. **Wait for it to fire.** The scheduler (`AlertEvaluationJob`, every 30 s,
   `config/recurring.yml:13-17`) picks up enabled rules whose `last_evaluated_at`
   is null or older than `evaluation_interval_seconds`
   (`app/jobs/alert_evaluation_job.rb:39-53`). With `for_duration_seconds: 300` the
   instance sits in `pending` until the condition holds that long, then flips to
   `alerting` (`state_manager.rb:166-201`). A missing metric, or a heartbeat older
   than 2 minutes, is an error, not a fire (`evaluation_service.rb:5`, `:68-69`).

   ```sh
   selfhost api -X GET /aws/v1/organizations/{org}/alert_instances --org acme \
     -f alert_rule_pid=alrt_… -f state=alerting -o json
   ```

5. **Acknowledge the fired instance** (mutating; valid only from `alerting`, and
   once — else 409, `alert_instances_controller.rb:52-59`):

   ```sh
   selfhost api -X POST /aws/v1/organizations/{org}/alert_instances/alinst_…/acknowledge --org acme
   ```

6. **Resolve it** (mutating) with an audit reason; the default is
   `"Manually resolved by <your name>"` (`alert_instances_controller.rb:91`):

   ```sh
   selfhost api -X POST /aws/v1/organizations/{org}/alert_instances/alinst_…/resolve \
     --org acme -f reason='drained the queue'
   ```

   Acknowledge and resolve do not stop the rule: the next evaluation can move the
   instance again (`state_manager.rb:207-230`).

7. **Tear down** (destructive). Deleting the rule cascades its instances and
   channel bindings (`app/models/alert_rule.rb:37-39`); the channel survives:

   ```sh
   selfhost api -X DELETE /aws/v1/organizations/{org}/alert_rules/alrt_… --org acme
   selfhost api -X DELETE /aws/v1/organizations/{org}/notification_channels/nchan_… --org acme
   ```

## Day-2 operations

| Task | Call |
|---|---|
| Disable a rule without losing its history | `PATCH …/alert_rules/<alrt_…>` with `{"alert_rule":{"enabled":false}}` — `alerting` instances are kept, everything else resets (`alert_rule.rb:79-100`) |
| Retarget a rule at another service | `PATCH …/alert_rules/<alrt_…>` with `"cloud_instance_pid"`; a replica is refused with 422 (`alert_rules_controller.rb:75-77`) |
| Silence a channel without deleting it | `PATCH …/notification_channels/<nchan_…>` with `{"notification_channel":{"enabled":false}}` |

## Gotchas

- **Writes are nested; `-f` cannot build them.** Rule create/update read
  `params.require(:alert_rule)` and channel create/update read
  `params.require(:notification_channel)` (`alert_rules_controller.rb:148`,
  `notification_channels_controller.rb:90`), and the CLI has no
  nested-parameter syntax (`references/api.md`). Always `--input -` with
  `{"alert_rule":{…}}` or `{"notification_channel":{…}}`. A flat body sent to a
  rule create/update fails as a 500 `An unexpected error occurred` (the action
  rescues the missing-wrapper error into a 500,
  `alert_rules_controller.rb:64-66`); the same mistake on a channel surfaces a
  clean 400.
- **Email is the only channel kind.** `VALID_CHANNEL_TYPES = %w[email]`
  (`app/models/notification_channel.rb:6`) and the only registered dispatch
  adapter is `email` (`app/services/notifications/dispatch_service.rb:5-7`). The
  `--kind` help text advertising slack/webhook/pagerduty is not backed by the
  backend: any other value is a 422 on create. `channel_config` must be
  `{"email":"…"}` and the address is format-checked and capped at 254 chars
  (`notification_channel.rb:44-58`); a missing key is 422 "must include an email
  address".
- **`channel_config` is returned, not hidden.** The serializer emits it verbatim
  on every read, and `test` sends to it. Do not print responses into logs.
- **`verified` is decorative today.** The column is serialized but no code path
  sets it and update does not permit it (`notification_channels_controller.rb:89-95`).
- **Evaluation cadence is two timers, not one.** The scheduler ticks every 30 s
  (`config/recurring.yml:17`), but a rule is only evaluated when
  `evaluation_interval_seconds` (default 60) has elapsed
  (`alert_evaluation_job.rb:39-53`), and `for_duration_seconds` (default 0)
  decides whether it fires immediately or via `pending`
  (`state_manager.rb:166-175`).
- **Notification cooldown, and silent rules.** `notification_cooldown_seconds`
  (default 300) suppresses repeats per instance; a `firing` inside it is dropped,
  while a `resolved` inside it is queued (`queued_notification_type`) and only
  delivered on the next notifier run after the cooldown expires — which happens
  solely on a later alerting/resolved transition
  (`app/services/alerting/notification_service.rb:30-46`,
  `app/jobs/evaluate_alert_rule_job.rb:52`). With no further transitions the
  queued `resolved` is never sent. A rule with no channel binding is silent
  whatever the cooldown: no binding means no dispatch and no
  log (`notification_service.rb:15-18`), so a green create is not a promise that
  anyone will be told.
- **`storage` metrics need `mount_point`.** Omitted → 422
  (`app/models/alert_rule.rb:50`). The seeder's defaults use `/`
  (`app/services/alerting/default_rules_seeder.rb:36-44`).
- **Instances cannot be created, patched or deleted over the API** — only the two
  transitions exist, and both demand `state == "alerting"`; anything else is 409,
  as is a second acknowledge of the same instance
  (`alert_instances_controller.rb:52-59`, `:87-89`). `reason` must be ≤ 500
  characters or the update is a 422 (`app/models/alert_instance.rb:15`).
- **`show` needs `index`.** `NotificationChannelsController` authorizes both
  `index` and `show` with `:index` (`notification_channels_controller.rb:10`), so
  a role without `notification_channels` cannot list or read a channel.
- **Rate limits are real 429s** — channel create 10/min, channel test 3/min, per
  user pid + remote IP (`notification_channels_controller.rb:16-22`).

## Sources

- Routes: `config/routes.rb:391-399`, nested under `namespace :aws` /
  `namespace :v1` (`:326-327`) and `resources :organizations` (`:386`).
- Controllers: `app/controllers/aws/v1/alert_rules_controller.rb`,
  `alert_instances_controller.rb`, `notification_channels_controller.rb`;
  organization resolution in `app/controllers/application_controller.rb:108-144`.
- Serializers: `app/serializers/{alert_rule,alert_instance,notification_channel}_serializer.rb`,
  `app/models/{alert_rule,alert_instance,notification_channel,notification_channel_binding}.rb`.
- Services/jobs: `app/services/alerting/{evaluation_service,state_manager,notification_service}.rb`,
  `app/services/notifications/{dispatch_service.rb,adapters/email_adapter.rb}`,
  `app/jobs/{alert_evaluation_job,evaluate_alert_rule_job}.rb`,
  `app/mailers/alert_mailer.rb`, `config/recurring.yml:13-17`.
- Metric names: `app/services/database_adapters/registry.rb` and the adapters'
  `valid_metrics` / `metric_type`.
