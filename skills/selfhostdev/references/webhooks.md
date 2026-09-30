# Webhooks

Customer webhook endpoints: the HTTPS URLs the platform POSTs deployment, preview,
scaling and credential events to. The typed commands answer `not implemented yet:
webhook <verb>` for every verb, so this work goes through `selfhost api` and the seven
routes below (five doing real work plus the two Rails scaffold form routes). One endpoint
with pid `whe_…` (`app/models/webhook_endpoint.rb:35`), owned by exactly one
organization (`app/models/webhook_endpoint.rb:17`) and scoped under
`/organizations/:organization_id/webhook_endpoints` (`config/routes.rb:256,285`).

These are **outbound** and customer-owned. The `webhooks` namespace at
`config/routes.rb:24-27` (`POST /webhooks/github`, `POST /webhooks/resend`) and
`/api/v1/razorpay/webhooks` (`config/routes.rb:173`) are **inbound** receivers for
GitHub, Resend and Razorpay: no user auth, the provider's own signature verified, not
customer configuration and not creatable through any endpoint in this file.

## Before you start

- `selfhost auth status` must be green; `--org` (or the profile's saved organization)
  must resolve to the organization that owns the endpoints.
- The path segment is matched with `Organization.find_by(pid: params[:organization_id])`
  (`app/controllers/webhook_endpoints_controller.rb:65`), so the path must carry the
  organization **pid**. Use the `{org}` placeholder — it is replaced with the resolved
  pid (`references/api.md`); a slug in the path 404s with
  `Organization not found or not specified.` (`app/controllers/application_controller.rb:168-172`).
- Calling rules are `references/api.md`: bare `api /path` is a GET, any `-f`/`-F`/`--input`
  turns it into a POST unless `-X` names the method, and `-X GET` is what keeps
  parameters on the query string. `--input -` sends stdin bytes verbatim as the body.

```sh
selfhost auth status
selfhost api /organizations/{org}/webhook_endpoints --org acme -o json
```

## Endpoint map

All rows resolve to `resources :webhook_endpoints` (`config/routes.rb:285`), nested in
`resources :organizations` (`config/routes.rb:256`). `:id` is the endpoint **pid**,
looked up with `find_by!(pid: params[:id])`
(`app/controllers/webhook_endpoints_controller.rb:77`).

| Future CLI command | Method and path | What it does | Key parameters | Source |
|---|---|---|---|---|
| `selfhost webhook list` | `GET /organizations/:organization_id/webhook_endpoints` | Read-only. Every endpoint of the organization. `pid` in the path | — | `config/routes.rb:285`, `app/controllers/webhook_endpoints_controller.rb:8-11` |
| `selfhost webhook show` | `GET /organizations/:organization_id/webhook_endpoints/:id` | Read-only. One endpoint | `id` = `whe_…` | `config/routes.rb:285`, `app/controllers/webhook_endpoints_controller.rb:13-15` |
| `selfhost webhook create` | `POST /organizations/:organization_id/webhook_endpoints` | Mutating. Creates one endpoint; the secret is generated server-side | `webhook_endpoint.url`, `webhook_endpoint.subscribed_events[]` | `config/routes.rb:285`, `app/controllers/webhook_endpoints_controller.rb:21-31`, `:81` |
| `selfhost webhook update` | `PATCH`/`PUT /organizations/:organization_id/webhook_endpoints/:id` | Mutating. Replaces `url` and/or `subscribed_events` | same two fields, both optional | `config/routes.rb:285`, `app/controllers/webhook_endpoints_controller.rb:38-47`, `:81` |
| `selfhost webhook delete` | `DELETE /organizations/:organization_id/webhook_endpoints/:id` | Destructive. Deletes the endpoint and its whole delivery history | `id` = `whe_…` | `config/routes.rb:285`, `app/controllers/webhook_endpoints_controller.rb:49-55`, `app/models/webhook_endpoint.rb:18` |
| — | `GET /organizations/:organization_id/webhook_endpoints/new` | Scaffold form route. Returns an unsaved blank endpoint, not useful for automation | — | `config/routes.rb:285`, `app/controllers/webhook_endpoints_controller.rb:17-19` |
| — | `GET /organizations/:organization_id/webhook_endpoints/:id/edit` | Scaffold form route. Returns the endpoint including the secret, same as show | `id` = `whe_…` | `config/routes.rb:285`, `app/controllers/webhook_endpoints_controller.rb:33-36` |

When the typed command lands, create/update are planned to take `--url <URL>` and a repeatable `--event <EVENT>` (`src/cli/webhook.rs:9-16`).

There is no route for `webhook_events` (delivery attempts): grep `config/routes.rb` for
`webhook_event` returns nothing. Delivery history is written to the database
(`app/services/webhook/trigger.rb:17-21`) but is not readable over HTTP.

## Register an endpoint end to end

1. **Write the create body to a file.** The controller requires the nested
   `webhook_endpoint` key (`app/controllers/webhook_endpoints_controller.rb:81`), and
   `selfhost api` has no nested-parameter syntax (`key[sub]` is a literal key,
   `references/api.md`), so a flat `-f` body cannot express it. Use `--input`.

   ```sh
   cat > /tmp/hook.json <<'JSON'
   {"webhook_endpoint":{"url":"https://hooks.example.com/selfhost",
                        "subscribed_events":["deploy_run.failed","deploy_run.succeeded"]}}
   JSON
   ```

   `subscribed_events` must be a non-empty array of names from `VALID_EVENTS`
   (`app/models/webhook_endpoint.rb:23,42-49`); `url` must be `https://` with a host
   (`app/models/webhook_endpoint.rb:51-60`).

2. **Create it** (mutating, no billing):

   ```sh
   selfhost api /organizations/{org}/webhook_endpoints --org acme -i --input /tmp/hook.json
   ```

   Confirmed by `HTTP 201`. The body of a create is a single JSON object, so the
   default output is `null` — see Gotchas.

3. **Read the endpoint back, secret included.** `index` answers with a JSON *array*,
   which `selfhost api` prints verbatim:

   ```sh
   selfhost api /organizations/{org}/webhook_endpoints --org acme -o json
   ```

   Each element carries `pid`, `url`, `subscribed_events`, `secret`, `created_at` and
   `updated_at` (`db/schema.rb:2053-2062`). Take the `whe_…` pid and the `whsec_…`
   secret of the row you just created and store them in your secret manager. The
   secret is 48 hex characters behind the `whsec_` prefix
   (`app/models/webhook_endpoint.rb:38-40`).

4. **Verify a delivery in your receiver.** Every delivery is
   `POST <url>` with `Content-Type: application/json`, `X-Webhook-Signature:
   <hex>` where the header is `OpenSSL::HMAC.hexdigest("SHA256", endpoint.secret,
   raw_body)` (`app/jobs/send_webhook_job.rb:31-32`). No timestamp, no version, no
   other header. The key is the secret **string as stored** — do not base64-decode it
   (unlike the inbound Svix/Resend path).

   ```sh
   # raw body as received, byte for byte
   printf '%s' "$RAW_BODY" | openssl dgst -sha256 -hmac "$WHSEC"
   # (stdin)= 4f2b8c…   must equal the X-Webhook-Signature header (compare constant-time)
   ```

   A delivery for the subscription above looks like this — field names verified at
   `app/services/webhook/deploy_run_events.rb:34-47`, values illustrative:

   ```
   POST /selfhost HTTP/1.1
   Content-Type: application/json
   X-Webhook-Signature: 4f2b8c1d…

   {"event":"deploy_run.failed","deployment_pid":"deploy_…","run_pid":"run_…",
    "repo_full_name":"acme/api","branch":"main","commit_sha":"9f1c…","source":"coolify",
    "status":"failed","error_message":"build exited 1","deploy_url":"https://app.example.com",
    "started_at":"2026-09-29T10:00:00Z","finished_at":"2026-09-29T10:02:11Z",
    "timestamp":"2026-09-29T10:02:11Z"}
   ```

5. **Change it** (mutating). `subscribed_events` is replaced, not merged, so send the
   full list you want:

   ```sh
   echo '{"webhook_endpoint":{"subscribed_events":["deploy_run.failed"]}}' \
     | selfhost api -X PATCH /organizations/{org}/webhook_endpoints/{id} \
         --org acme --input -
   ```

   Pass the pid literally (drop `{org}`/`{id}` for real values) — only `{org}` is a
   placeholder (`references/api.md`).

6. **Delete it** (destructive, cascades to the delivery rows):

   ```sh
   selfhost api -X DELETE /organizations/{org}/webhook_endpoints/{id} --org acme
   ```

   Answers `HTTP 200` with the envelope `{"status":"success","data":null,…}`
   (`app/controllers/webhook_endpoints_controller.rb:54`), so the CLI prints `null`.

## Event catalog

`VALID_EVENTS` is the only accepted vocabulary (`app/models/webhook_endpoint.rb:6-14`),
and `Webhook::Trigger` delivers to an endpoint only when its list contains the event
name (`app/services/webhook/trigger.rb:15`). A name that is not in `VALID_EVENTS` is
rejected at create/update with `contains unknown event(s): …`
(`app/models/webhook_endpoint.rb:45-48`); an event that is not in `VALID_EVENTS` can
never be subscribed.

| Event | Fires when | Payload fields | Source |
|---|---|---|---|
| `instance.scaling_failed` | An in-place resize fails | `instance_pid`, `from_instance_type`, `target_instance_type`, `error`, `scaling_event_id`, `timestamp` | `app/services/instance_scaling/failure_handler_service.rb:64-75` |
| `deploy_run.succeeded` | Coolify reports the build finished successfully | `event`, `deployment_pid`, `run_pid`, `repo_full_name`, `branch`, `commit_sha`, `source`, `status`, `error_message`, `deploy_url`, `started_at`, `finished_at`, `timestamp` | `app/services/webhook/deploy_run_events.rb:13,32-47` |
| `deploy_run.failed` | The build failed | same as above | `app/services/webhook/deploy_run_events.rb:14,32-47` |
| `deploy_run.aborted` | The build was cancelled | same as above | `app/services/webhook/deploy_run_events.rb:15,32-47` |
| `preview_deployment.created` | A PR preview was provisioned | `event`, `deployment_pid`, `pr_number`, `repo_full_name`, `branch`, `deploy_url`, `parent_deployment_pid`, `timestamp` | `app/services/webhook/preview_deployment_events.rb:13,30-41` |
| `preview_deployment.destroyed` | A PR preview was torn down | same as above | `app/services/webhook/preview_deployment_events.rb:14,30-41` |
| `database_user.password_rotated` | Opted-in scheduled password rotation ran | `instance_pid`, `database_user_pid`, `username`, `reason` (`scheduled_rotation`), `timestamp`; the new password is deliberately absent | `app/jobs/database_users/database_user_password_expiry_job.rb:126-135` |

Only the `deploy_run.*` and `preview_deployment.*` payloads carry an `event` key; the
two others leave the name to the subscription. There is no wildcard and no
subscribe-to-all: `subscribed_events` must be a non-empty list
(`app/models/webhook_endpoint.rb:23`).

## Delivery, retries and signing

- Delivery runs asynchronously on solid_queue (`config/application.rb:74`). The
  trigger writes a `WebhookEvent` row with `status: "pending"` and enqueues
  `SendWebhookJob` (`app/services/webhook/trigger.rb:17-23`).
- The job POSTs the payload JSON to the stored `url` and sets `status` to `success`
  on 2xx or `failed` otherwise, recording `{code:, body:}` or `{error:}` in the
  `response` column (`app/jobs/send_webhook_job.rb:37-44`, `db/schema.rb:2063-2072`).
- Failures retry: `retry_on StandardError, wait: 10.seconds, attempts: 5`
  (`app/jobs/send_webhook_job.rb:3`) — up to 5 attempts, ~10 s apart, the same body
  every time (there is no idempotency key, so deduplicate on `event` + the resource
  pid). After the last attempt the job is dead in the queue backend.
- No auto-disable: nothing in the model, the job or the trigger disables an endpoint
  after repeated failures; a broken subscriber keeps producing failed `WebhookEvent` rows.

## Day-2 operations

| Goal | How |
|---|---|
| Add or drop events | `PATCH` the endpoint with the full `subscribed_events` array (step 5) |
| Change the URL | `PATCH` with `webhook_endpoint.url` |
| Rotate the signing secret | No rotation endpoint; delete and recreate the endpoint, then redeploy the receiver with the new `secret` |
| Test that a URL is reachable | No test-fire endpoint exists — subscribe to a cheap event and trigger it, or temporarily point the endpoint at a request bin |
| Read delivery history | Not available over HTTP (no `webhook_events` route); log the deliveries your receiver gets |

## Gotchas

- **The secret is not a one-time reveal.** `WebhookEndpoint` has no serializer and no
  `as_json` override (`app/serializers/` has no webhook file; the model is 61 lines
  and ends at `app/models/webhook_endpoint.rb:61`), and the controller renders the record directly
  (`render json: @webhook_endpoint`, `app/controllers/webhook_endpoints_controller.rb:10,14,27,43`).
  So `secret` — and `organization_pid`, which this record also exposes although other
  models trim it in `as_json` (`app/models/org_ssh_key.rb:41-43`) — is in those bodies,
  encrypted **at rest** (`app/models/webhook_endpoint.rb:26`) but cleartext on the wire.
  Never paste `api` output into logs or a conversation (`references/api.md`).
- **`show`, `create` and `update` print `null`.** They answer with a bare JSON object,
  the CLI reads only the envelope's `data` field, and an object without `data` becomes
  `null` (`src/api/mod.rs:444-448`) — `-i` does not help, it renders the same `data`;
  it only adds the useful `HTTP 201` line. `index` prints real output because a JSON
  *array* is not treated as an envelope (`src/api/mod.rs:481-500`). Read the created
  endpoint back with `list`.
- **The create body must be nested.** `params.require(:webhook_endpoint)` means a flat
  `-f url=… -f subscribed_events=…` body fails with a 400 error envelope; there is no
  `key[sub]` syntax in `selfhost api`. Use `--input`.
- **HTTPS only.** `http://`, a missing host or an unparseable URL is a 422
  (`url: must be a valid HTTPS URL`, `app/models/webhook_endpoint.rb:51-60`). The
  delivery job re-checks the scheme and marks the event failed without an HTTP call
  if the stored URL is not https (`app/jobs/send_webhook_job.rb:15-18`).
- **`subscribed_events` is replaced, not merged.** A `PATCH` carrying the array
  overwrites it; an empty array fails the presence validation
  (`app/models/webhook_endpoint.rb:23`), so you cannot clear the list — delete the
  endpoint instead.
- **Writes need a role; reads need only membership.** `create`/`update`/`destroy` go
  through `authorize!(:webhook_endpoints, …)`
  (`app/controllers/webhook_endpoints_controller.rb:4-6`), which only `owner`
  (`all: true`), `admin` and `manager` hold (`db/seeds.rb:25,51`); `member` and
  `billing` get 403 `You do not have permission to perform this action.`
  (`app/controllers/concerns/authorization_concern.rb:50-53`). `index` and `show` have
  no such gate, so any active member can read the endpoints — and therefore the
  secrets.
- **Wrong org → 404, not 403.** A non-member on a valid organization pid is dropped
  to `@current_organization = nil` before the action
  (`app/controllers/application_controller.rb:140-142`) and answers 404
  `Organization not found or not specified.`; tests assert the other tenant's secret is
  never in the body (`test/controllers/resource_authorization_test.rb:237-249`).
- **Org injection is harmless here but can be forced.** The CLI appends the resolved
  `organization_id` to the query when the path already carries it; pass
  `-f organization_id=<pid>` to suppress injection if you must hardcode the path pid
  (`references/api.md`).
- **Deleting an endpoint deletes its delivery rows** (`dependent: :destroy`,
  `app/models/webhook_endpoint.rb:18`), and deleting the organization deletes the
  endpoints (`app/models/organization.rb:40`).

## Sources

- `config/routes.rb:23-27` (inbound `webhooks` namespace), `:173` (inbound Razorpay),
  `:256` (`resources :organizations`), `:285` (`resources :webhook_endpoints`)
- `app/controllers/webhook_endpoints_controller.rb` (whole file: callbacks 2-6, actions
  8-55, `set_organization` 59-74, `set_webhook_endpoint` 76-78, strong params 80-82)
- `app/models/webhook_endpoint.rb` (whole file), `app/models/webhook_event.rb:8-12,23,31`
- `app/jobs/send_webhook_job.rb` (whole file), `app/services/webhook/trigger.rb:3-25`
- `app/services/webhook/deploy_run_events.rb`, `app/services/webhook/preview_deployment_events.rb`
- `app/jobs/database_users/database_user_password_expiry_job.rb:122-136`,
  `app/services/instance_scaling/failure_handler_service.rb:61-75`
- `db/schema.rb:2053-2072`, `db/seeds.rb:25,51`,
  `db/migrate/20260818090000_add_platform_permissions_to_roles.rb`
- `app/controllers/application_controller.rb:108-173`,
  `app/controllers/concerns/authorization_concern.rb:25-53`
- `test/controllers/resource_authorization_test.rb:237-285`,
  `test/controllers/role_enforcement_test.rb:119-130,199-215`,
  `test/models/webhook_endpoint_test.rb`
- CLI side: `src/cli/webhook.rs:9-35`, `src/cli/api.rs:80-130,330-380`, `src/api/mod.rs:444-500`
