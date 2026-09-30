# Cloud credentials and the cloud account

Bring-your-own-cloud keys for `selfhost cloud credential …` — every verb is
still `not implemented yet`, so use `selfhost api` with the paths below. A
credential is one provider's keys belonging to one organization, pid
`ccred_<32 hex>` (`app/models/cloud_credential.rb:152`), stored encrypted
(`cloud_credential.rb:34`). All credential routes hang off
`/organizations/:organization_id/…` (`config/routes.rb:281-283`, nested in
`resources :organizations` at `config/routes.rb:256`). There is **no** cloud
account endpoint: `cloud account show` is the org's default credential plus its
account info, both below.

## Before you start

- `selfhost auth status` — a signed-in profile. Every call is org-scoped, so a
  resolved organization is required (`--org <slug>` or `selfhost org use <slug>`).
- Read `references/api.md` for the calling rules. The ones that bite here:
  - the path segment must be the org **pid**; `--org acme` works because the CLI
    substitutes the resolved pid for `{org}` (`references/api.md`, "The `{org}`
    placeholder").
  - create and update need a **nested** JSON body, and `-f`/`-F` keys are literal
    (`key[sub]` is not nesting), so they cannot build it — use `--input`.
  - `--input` alone makes the request a POST; an update needs an explicit
    `-X PATCH`, and `set_default`/`delete` need `-X POST` / `-X DELETE` because a
    bare path is a GET. No GET in this domain takes parameters, so none needs
    `-X GET` to stay a GET.
  - secrets: pass a key pair through a `--input` file (mode `600`), never a flag
    value that lands in shell history or the process list.

## Endpoint map

All paths below are relative to `{org}` = the organization pid.

| Future CLI command | Method and path | What it does | Key parameters | Source |
|---|---|---|---|---|
| `selfhost cloud credential list` | `GET /organizations/{org}/cloud_credentials` | Read-only. The org's credentials **and** every platform-managed one, ordered platform-managed, then default, then name. `data` is a bare array; no pagination parameters | none | `config/routes.rb:281`, `app/controllers/cloud_credentials_controller.rb:7`, `cloud_credential.rb:64,74` |
| (no verb yet) | `GET /organizations/{org}/cloud_credentials/:id` | Read-only. Re-read one credential | path `id` (`ccred_…`) | `config/routes.rb:281`, `cloud_credentials_controller.rb:18` |
| `selfhost cloud credential add` | `POST /organizations/{org}/cloud_credentials` | Mutating. Create a BYOC credential. For `aws` + `access_key` the pair is proved against AWS STS before anything is stored | body `cloud_credential{name, cloud_provider, auth_method, credential_data{access_key_id, secret_access_key}, description, pitr_s3_bucket, metadata}` | `config/routes.rb:281`, `cloud_credentials_controller.rb:26,202`, `cloud_credential.rb:50-56` |
| `selfhost cloud credential update` | `PATCH /organizations/{org}/cloud_credentials/:id` (PUT also routes) | Mutating. Rename, rotate keys, change `description`/`pitr_s3_bucket`/`metadata`. `cloud_provider` and `auth_method` are immutable | body `cloud_credential{…}` as create | `config/routes.rb:281`, `cloud_credentials_controller.rb:63,231`, `cloud_credential.rb:190` |
| `selfhost cloud credential delete` | `DELETE /organizations/{org}/cloud_credentials/:id` | Destructive (soft). Sets `deleted_at`; the row leaves the list | path `id` | `config/routes.rb:281`, `cloud_credentials_controller.rb:103`, `app/models/application_record.rb:8` |
| `selfhost cloud credential default` | `POST /organizations/{org}/cloud_credentials/:id/set_default` | Mutating. Clears `is_default` on the org's other credentials for the **same provider**, then sets this one | path `id` | `config/routes.rb:282`, `cloud_credentials_controller.rb:168`, `cloud_credential.rb:111` |
| part of `selfhost cloud account show` | `GET /organizations/{org}/cloud_credentials/:id/account_info` | Read-only, live AWS call. `account_id` + free tier plan for the stored keys | path `id` | `config/routes.rb:283`, `cloud_credentials_controller.rb:135` |

Credential object in `data` (`app/serializers/cloud_credential_serializer.rb:2`):
`id`, `name`, `description`, `cloud_provider`, `auth_method`, `status`
(`active|invalid|expired|disabled`, `cloud_credential.rb:9`), `is_default`,
`is_platform_managed`, `last_validated_at`, `validation_error`, `metadata`,
`pitr_s3_bucket`, `data_directory` (constant `/selfhostdev/postgresql/`,
serializer `:22`), `created_at`, `updated_at`, `created_by`/`updated_by`
(`{pid, email, display_name}` or null). `credential_data` is deliberately absent
(serializer `:20`).

`account_info` `data` is `{account_id, free_tier}` where `free_tier` is
`{account_plan_type, account_plan_status, account_plan_expiration_date,
account_plan_remaining_credits}` (`app/services/cloud_provider/aws/concerns/aws_account_resolver.rb:63`),
plus `free_tier_error` when the plan could not be read
(`cloud_credentials_controller.rb:158-159`).

### Who may call what

`create`, `update`, `destroy`, `set_default` and `account_info` require an
**owner or admin** membership — anyone else gets 403 `You must be an owner or
admin to perform this action` (`cloud_credentials_controller.rb:5,258`).
`index` and `show` are open to any member of the org.

### `cloud account show`

There is no account route. Compose it: list credentials and take the
organization's own row marked as the default for the provider you care
about, then call `account_info` on it. There is no fallback to the shared
platform row — `account_info` on a platform-managed credential always
answers 422 `Account info is not available for platform-managed
credentials`, so when the organization has no default of its own there is
no account to show. `account_info` proves the stored keys still work: it
calls STS `GetCallerIdentity` in `us-east-1` at request time
(`aws_account_resolver.rb:47`) and returns the account they belong to.

## Link one cloud account end to end

1. **Add the credential** (mutating, owner/admin). The body must be wrapped in
   `cloud_credential`; the CLI's org injection rides on the query string for an
   `--input` request, so the file holds only this object:

   ```sh
   cat > /tmp/cred.json <<'JSON'
   {"cloud_credential":{"name":"qa-aws","cloud_provider":"aws","auth_method":"access_key",
     "credential_data":{"access_key_id":"AKIA…","secret_access_key":"…"}}}
   JSON
   chmod 600 /tmp/cred.json
   selfhost api /organizations/{org}/cloud_credentials --org acme --input /tmp/cred.json
   ```

   Keep `data.id` (`ccred_…`). `data.status` is `active`,
   `data.last_validated_at` is the verification time and
   `data.metadata.aws_account_id` is the account STS reported
   (`cloud_credentials_controller.rb:302-307`). **No response ever contains
   `credential_data`** — there is no one-time echo, so the secret cannot be read
   back; keep your own record of the access key id, never of the secret.

2. **Verify it independently** (read-only, but it does call AWS):

   ```sh
   selfhost api /organizations/{org}/cloud_credentials/ccred_…/account_info \
     --org acme -o json
   ```

   `data.account_id` must be the 12-digit account the keys belong to. A missing
   `data.free_tier` with `data.free_tier_error` means the keys work but lack
   `freetier:GetAccountPlanState` — not a failure of the credential.

3. **Make it the default** (mutating, owner/admin). The flag is only a
   label for your own rows — provisioning never reads it:

   ```sh
   selfhost api -X POST /organizations/{org}/cloud_credentials/ccred_…/set_default --org acme
   ```

   Re-list and confirm only that credential carries `is_default: true` for its
   provider among the org's own rows.

4. **Rotate later** (mutating; owner/admin). The `PATCH` is explicit — with
   `--input` the default method would be POST:

   ```sh
   cat > /tmp/rotate.json <<'JSON'
   {"cloud_credential":{"credential_data":{"access_key_id":"AKIA…","secret_access_key":"…"}}}
   JSON
   chmod 600 /tmp/rotate.json
   selfhost api -X PATCH /organizations/{org}/cloud_credentials/ccred_… \
     --org acme --input /tmp/rotate.json
   ```

   The new pair is verified before the old one is replaced. The response
   `message` names any active instances that will **not** pick up the new keys.

5. **Delete it** (destructive; owner/admin). Refused while any instance
   references it:

   ```sh
   selfhost api -X DELETE /organizations/{org}/cloud_credentials/ccred_… --org acme
   ```

   `data` is `null`. Finish with `shred -u /tmp/cred.json /tmp/rotate.json`.

## Day-2 operations

| Task | Call |
|---|---|
| Find the credential a database create will use | pass its id as `cloud_credential_id` on the create call; when omitted, the create falls back to the shared platform-managed credential, and Hetzner project servers never read stored credentials at all (they use the server-side Hetzner key) |
| Confirm the keys still work | `GET …/cloud_credentials/<ccred_…>/account_info` — live STS call |
| Turn a credential off temporarily | no flag for this; it is `set_default` on another credential of the same provider |
| Rotate a leaked key | `PATCH …/cloud_credentials/<ccred_…>` with a new `credential_data`, then re-run `account_info` |
| Detach a storage bucket | `PATCH` with `pitr_s3_bucket` (create accepts it too) |
| Remove a credential | `DELETE …/cloud_credentials/<ccred_…>` — soft delete, blocked while instances reference it |

## Gotchas

- **The body is wrapped.** `params.require(:cloud_credential)` is called in both
  create and update (`cloud_credentials_controller.rb:202,231`); a flat body is
  a 400 `param is missing or the value is empty: cloud_credential`. Since
  `-f`/`-F` keys are literal and cannot create the nesting, `--input` is the
  only way to send either request. Inside the wrapper, `credential_data` may be
  an object **or** a JSON string — the controller parses a string
  (`:219`) — but the wrapper itself is mandatory.
- **Secrets are write-only.** `credential_data` is encrypted at rest
  (`cloud_credential.rb:34`) and excluded from the serializer, so create, show
  and update all return it absent. `--debug` redacts the request only; `-i`
  prints the response verbatim. Never paste credential files or output anywhere.
- **Provider values map 1:1.** `selfhost cloud credential add --provider` takes
  `aws` or `hetzner` (`src/cli/mod.rs:107`), the same two strings the model
  accepts (`cloud_credential.rb:5,50`, from
  `app/services/cloud_provider/registry.rb:3`). But only **`aws` +
  `access_key`** is consumed: every reader of `credential_data` is on the AWS
  path, and the Hetzner client authenticates with `ENV["HETZNER_API_KEY"]`
  instead (`app/services/cloud_provider/hetzner/client.rb:20`). A `hetzner`
  credential is stored and listed but changes nothing — do not expect it to
  link a Hetzner account.
- **`auth_method` is effectively one value here.** The column accepts
  `access_key`, `platform_managed` and `iam_role` (`cloud_credential.rb:8`), but
  only `access_key` is ever read (`cloud_credentials_controller.rb:317-328`);
  `platform_managed` is unreachable through this API because create forces
  `is_platform_managed = false` (`:30`). Key-shape validation (`access_key_id`
  plus `secret_access_key`) also only runs for `aws` + `access_key`
  (`cloud_credential.rb:167-186`), yet a non-empty `credential_data` is required
  whenever the row is not platform-managed **and** its auth method is not
  `platform_managed` (`cloud_credential.rb:161`) — so a row saved with
  auth_method `platform_managed` can carry blank keys.
- **Create and rotate verify against AWS.** `CloudProvider::Aws::CredentialVerifier`
  calls STS `GetCallerIdentity` in `us-east-1`
  (`app/services/cloud_provider/aws/credential_verifier.rb:19,42`). AWS saying
  no is a 422 with code `AWS_KEYS_REJECTED` and field `credential_data`; AWS
  being unreachable is a **502** with code `AWS_UNREACHABLE` — retry that one,
  the keys may be fine (`cloud_credentials_controller.rb:309-314`). Either way
  nothing is stored.
- **`account_info` is not provider-checked.** It only requires
  `auth_method == "access_key"` and both key halves
  (`cloud_credentials_controller.rb:317-328`), so a `hetzner` credential holding
  AWS-shaped data is sent to AWS STS. Platform-managed credentials answer 422
  `Account info is not available for platform-managed credentials`;
  a credential that cannot fetch an account id answers 502
  (`:136,150`). `free_tier` is `nil` when the keys lack
  `freetier:GetAccountPlanState`.
- **One default per org *and* provider.** `set_as_default!` clears `is_default`
  on the other rows sharing `organization_pid` + `cloud_provider`
  (`cloud_credential.rb:111-125`); there is no global default and no unset
  action. A platform-managed credential can never be made default (403).
  The flag is only a label: nothing in provisioning reads it, and the
  `default_for` helper that falls back to the platform-managed row
  (`cloud_credential.rb:83`) has no callers — instance creates use the
  `cloud_credential_id` they are given, else the platform-managed credential.
- **`cloud_provider` and `auth_method` are immutable.** Changing either on
  update is a 422 naming that attribute (`cloud_credential.rb:190-200`); a
  different provider is a new credential.
- **Delete is soft and refuses in-use rows.** `soft_delete!` only writes
  `deleted_at` (`app/models/application_record.rb:8`), and the guard is
  `cloud_instances.any?` (`cloud_credentials_controller.rb:106`) on an
  **unscoped** association (`cloud_credential.rb:29`) — instances that are
  themselves soft-deleted still block the delete with 422
  `Cannot delete credential that is in use by cloud instances`.
- **Rotating keys does not propagate.** Keys are embedded into each instance's
  boot-time config, so an update while instances exist succeeds and the response
  `message` lists the affected instance pids for manual reconfiguration
  (`cloud_credentials_controller.rb:274-284`).
- **Platform-managed rows share the namespace.** They appear in every org's list
  (ordered first) via the `for_organization` scope
  (`cloud_credential.rb:64-67`), but `update`, `destroy` and `set_default`
  reject them with 403 `Platform-managed credentials cannot be …`
  (`cloud_credentials_controller.rb:64,104,169`). Treat them as read-only and
  never as the org's own account.
- **Org scoping is pid-only.** The path segment *is* `params[:organization_id]`,
  and it is resolved with `Organization.find_by(pid: …)`
  (`app/controllers/application_controller.rb:123`), so a slug in the path is a
  404 `Organization not found or not specified.` (`:168`) — a slug is never
  looked up. The CLI's injected `organization_id` cannot rescue it: Rails merges
  path parameters **over** query parameters for the same key
  (`params.merge!(path_parameters)`), so on these nested routes the segment wins
  and the injected org is shadowed. A credential id from another org also 404s
  (`cloud_credentials_controller.rb:195`).

## Sources

- Routes: `config/routes.rb:256` (organizations), `:281-283` (cloud credentials,
  set_default, account_info).
- Controller: `app/controllers/cloud_credentials_controller.rb` (all 7 actions),
  `app/controllers/application_controller.rb:123,168` (org resolution),
  `app/controllers/concerns/response_handler.rb:7` (envelope).
- Model: `app/models/cloud_credential.rb`,
  `app/models/application_record.rb:8` (soft delete).
- Serializer: `app/serializers/cloud_credential_serializer.rb`.
- Services: `app/services/cloud_provider/aws/credential_verifier.rb`,
  `app/services/cloud_provider/aws/concerns/aws_account_resolver.rb`,
  `app/services/cloud_provider/registry.rb`,
  `app/services/cloud_provider/hetzner/client.rb:20`.
- CLI: `src/cli/cloud.rs` (verbs and flags), `src/cli/mod.rs:107` (`Provider`).
