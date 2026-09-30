# SSH keys

Three resources behind `selfhost ssh-key …`, all still `not implemented yet`:
the **organization key library** (`/api/v1/platform/ssh_keys`, pid `orgkey_…`),
the **keys installed on one project's server**
(`/api/v1/platform/projects/:project_pid/ssh_keys`, pid `prjkey_…`), and the
**SSH access policy** for that server (`…/ssh_access`) — which source IPs may
reach port 22. Only PUBLIC keys are stored or transported. The project pid is
`prj_…` and comes from `projects.md`.

Org-library keys are org-scoped: the controller reads `organization_id` from
params (`app/controllers/api/v1/platform/org_ssh_keys_controller.rb:63`), which
the CLI injects from the resolved organization, or you pass with `-f`. Project
sub-resources are not; the project pid already fixes the org.

## Before you start

- `selfhost auth status` — a signed-in profile. Org-library calls additionally
  need a resolved organization (`--org <slug>` or `selfhost org use <slug>`):
  `--org` accepts a slug and the CLI turns it into the pid the controller wants.
- Read `references/api.md` for the calling rules. The ones that bite here:
  a get with parameters needs `-X GET` (otherwise `-f`/`-F` makes it a POST);
  `-f` sends raw strings, `-F` types them; `-F key=@file` reads a file as one
  string; anything with an **array** (`cidrs`) must go through `--input -` with a
  JSON body — two `-f cidrs=…` items do not build an array.
- Public key format is enforced; see Gotchas.

## Endpoint map

### Organization key library (org-scoped)

| Future CLI command | Method and path | What it does | Key parameters | Source |
|---|---|---|---|---|
| `selfhost ssh-key org list` | `GET /api/v1/platform/ssh_keys` | List the org's keys, newest first | `organization_id` (injected) | `config/routes.rb:101`, `app/controllers/api/v1/platform/org_ssh_keys_controller.rb:25` |
| `selfhost ssh-key org add` | `POST /api/v1/platform/ssh_keys` | Save a public key in the library | `name`, `public_key` | `config/routes.rb:101`, `app/controllers/api/v1/platform/org_ssh_keys_controller.rb:32` |
| `selfhost ssh-key org remove` | `DELETE /api/v1/platform/ssh_keys/:pid` | Delete a library key (cascades project rows) | path `pid` (`orgkey_…`) | `config/routes.rb:101`, `app/controllers/api/v1/platform/org_ssh_keys_controller.rb:52` |

`POST` and `DELETE` need the `projects:create` / `projects:destroy` platform
role; `GET` is open to any org member
(`org_ssh_keys_controller.rb:21-22`). `:pid` is `orgkey_<ulid>`.

### Keys on one project's server

| Future CLI command | Method and path | What it does | Key parameters | Source |
|---|---|---|---|---|
| `selfhost ssh-key project list` | `GET /api/v1/platform/projects/:project_pid/ssh_keys` | List installed/queued keys + current access source | path `project_pid` | `config/routes.rb:92`, `app/controllers/api/v1/platform/project_ssh_keys_controller.rb:26` |
| `selfhost ssh-key project add` | `POST /api/v1/platform/projects/:project_pid/ssh_keys` | Add a key by library reference or inline; optional source change | `org_ssh_key_pid` **or** `public_key` (`name` optional, defaults to a generated name); optional `source`, `cidrs`, `access_level` | `config/routes.rb:93`, `project_ssh_keys_controller.rb:44` |
| `selfhost ssh-key project remove` | `DELETE /api/v1/platform/projects/:project_pid/ssh_keys/:pid` | Remove a key from the project | path `pid` (`prjkey_…`) | `config/routes.rb:94`, `project_ssh_keys_controller.rb:146` |

`POST`/`DELETE` need `projects:create` / `projects:destroy`; listing is a member
read (`project_ssh_keys_controller.rb:21-23`). A `source` in the create body
additionally requires `projects:update` (`project_ssh_keys_controller.rb:50`).

### SSH access policy (port 22)

| Future CLI command | Method and path | What it does | Key parameters | Source |
|---|---|---|---|---|
| (part of `ssh-key project list`) | `GET /api/v1/platform/projects/:project_pid/ssh_access` | Show the current source mode and CIDRs | path `project_pid` | `config/routes.rb:97`, `app/controllers/api/v1/platform/project_ssh_access_controller.rb:25` |
| `selfhost ssh-key access set` | `PUT /api/v1/platform/projects/:project_pid/ssh_access` | Replace the source mode and its CIDRs | `source`; `cidrs` for `custom` | `config/routes.rb:98`, `project_ssh_access_controller.rb:32` |

`PUT` needs `projects:update` (`project_ssh_access_controller.rb:22`).

## Provision one end to end

1. **Read the project pid** — `selfhost api /api/v1/platform/projects -o json`
   (`projects.md`). Everything below is `prj_…`.

2. **List the library** (read-only; the org comes from `--org`/profile):

   ```sh
   selfhost api /api/v1/platform/ssh_keys -o json
   ```

   Response `data.ssh_keys[]` — each has `pid`, `name`, `public_key`,
   `fingerprint`, `added_by_user_pid`, `created_at`, `platform_generated`
   (`app/models/org_ssh_key.rb:42`).

3. **Upload a key** (mutating, needs `projects:create`). `-F key=@file` inserts
   the file contents as one string; the model strips the trailing newline and
   the comment:

   ```sh
   selfhost api /api/v1/platform/ssh_keys \
     -f name=laptop -F public_key=@$HOME/.ssh/id_ed25519.pub
   ```

   Keep `data.ssh_key.pid` (`orgkey_…`) — that is the reference for step 4.

4. **Attach it to the project** (mutating). If the server is already live the key
   is appended to the `shell` user's `authorized_keys` immediately; if it is
   still provisioning the row is *queued* and installed when the agent comes up:

   ```sh
   selfhost api /api/v1/platform/projects/prj_…/ssh_keys \
     -f org_ssh_key_pid=orgkey_…
   ```

   Or paste a key straight in — it is also saved to the org library, and a
   duplicate fingerprint reuses the existing library key:

   ```sh
   selfhost api /api/v1/platform/projects/prj_…/ssh_keys \
     -f name=laptop -F public_key=@$HOME/.ssh/id_ed25519.pub
   ```

   `data` is the same shape as the list: `ssh_command`, `public_ip`,
   `ssh_keys[]`, `source`, `cidrs`, `requester_ip`
   (`project_ssh_keys_controller.rb:195`).

5. **Set the access policy** (mutating). Anywhere — clears the CIDR list:

   ```sh
   selfhost api -X PUT /api/v1/platform/projects/prj_…/ssh_access -f source=anywhere
   ```

   My IP — the server records **your** address; you cannot pass it, and a
   client-supplied `cidrs` is ignored:

   ```sh
   selfhost api -X PUT /api/v1/platform/projects/prj_…/ssh_access -f source=my_ip
   ```

   Custom — an array, so use a JSON body. CIDRs are canonicalized server-side
   (bare IPv4 address → `/32`, bare IPv6 address → `/128`, host bits zeroed):

   ```sh
   echo '{"source":"custom","cidrs":["203.0.113.5/32","10.0.0.0/8"]}' \
     | selfhost api -X PUT /api/v1/platform/projects/prj_…/ssh_access --input -
   ```

   Response `data` — `source`, `cidrs`, `requester_ip`, and on the write only
   `applied` (bool). `apply_note` shows up only when there is something to
   report: a successful reconcile returns `applied: true` with no `apply_note`
   key at all (`project_ssh_access_controller.rb:78-86`; a GET omits both,
   since `applied: nil` is compact-ed away). `applied:false` has two meanings
   (`app/services/coolify/host/ssh_source.rb:49-58`): the project is not
   `active`/`stopped`, so the note says it will apply at provisioning and
   **no job is enqueued**; or a live reconcile failed, so
   `Coolify::ReconcileFirewallJob` was enqueued and the note says it is being
   retried. `apply_note` is the only way to tell them apart — a re-read cannot.

6. **Connect** — `data.ssh_command` is `ssh shell@<public_ip>`, the non-root
   `shell` user (no docker group, no sudo). Root is reserved for platform keys.

## Day-2 operations

| Task | Call |
|---|---|
| Add another key to a live server | `POST …/ssh_keys` — agent appends it; `status` becomes `installed` |
| Grant write access | `POST …/ssh_keys -f org_ssh_key_pid=… -f access_level=write` — queued `pending_approval`, no install |
| Widen/narrow reach | `PUT …/ssh_access` with `source` + `cidrs` |
| Remove one key | `DELETE …/ssh_keys/<prjkey_…>` — rewrites `authorized_keys` without it |
| Retire a library key | `DELETE /api/v1/platform/ssh_keys/<orgkey_…>` — see Gotchas first |

## Gotchas

- **Key format is enforced.** Only
  `ssh-ed25519`, `ssh-rsa`, `ecdsa-sha2-nistp256|384|521`,
  `sk-ssh-ed25519@openssh.com` (`app/models/org_ssh_key.rb:13`). A `PRIVATE KEY`
  paste is rejected by name. The key is normalized to `"<algo> <base64>"` with
  the comment stripped, and the blob's inner algorithm must match the prefix
  (`org_ssh_key.rb:53`, `org_ssh_key.rb:71`). Fingerprints are OpenSSH style —
  `SHA256:<base64, no padding>` (`org_ssh_key.rb:34`).
- **Duplicate keys.** `fingerprint` is unique per organization
  (`org_ssh_key.rb:30`): a second org-library `POST` of the same key is a 422
  "is already in this organization's key library". Project adds dedupe by
  `org_ssh_key_pid` per project (`app/models/coolify_project_ssh_key.rb:30`) —
  also 422.
- **Key cap.** At most 10 keys per project
  (`coolify_project_ssh_key.rb:10`); the 11th is a 422.
- **Access-policy values.** `source` must be `anywhere`, `my_ip` or `custom`
  (`app/models/coolify_project.rb:55`); anything else is a 422 naming the
  allowed set. `cidrs` is honored **only** for `custom` — `anywhere` clears it
  to `[]`, `my_ip` replaces it with the requester's single address
  (`app/services/coolify/host/ssh_source.rb:18`). Restricted modes demand at least
  one CIDR, at most 10, canonical form, and reject `/0`
  (`coolify_project.rb:296-323`). A restricted write is refused outright while
  the platform's own egress cannot be resolved — 422 "We can't verify the
  platform's own address…".
- **Removal side effects.** Deleting a library key cascades its project rows
  (`dependent: :destroy`) but does **not** strip the key from live servers — the
  material stays in their `authorized_keys` (`org_ssh_keys_controller.rb:49`).
  Because the project rows are gone, you can no longer remove it per project;
  prefer removing project installs first. Project `DELETE` only calls the agent
  when the key is `installed` **and** the agent is reachable
  (`project_ssh_keys_controller.rb:146`); with the agent down the row is
  destroyed and the key stays on the box.
- **`write` access is a gate, not a capability.** `access_level: write` queues
  the key `pending_approval` and installs nothing until a platform admin
  approves it (`/admin/ssh-access-requests`, `config/routes.rb:203`); it lands on
  the same `shell` user as `read_only`.
- **`my_ip` is server-observed.** The requester address in the project key and
  access-policy responses is the address the server saw (Cloudflare-aware),
  never one you sent. Org-library responses carry no requester address.

## Sources

- Routes: `config/routes.rb:33` (projects), `:92-98` (project ssh keys +
  ssh_access), `:101-102` (org key library), `:203` (admin approval queue).
- Controllers: `app/controllers/api/v1/platform/org_ssh_keys_controller.rb`,
  `project_ssh_keys_controller.rb`, `project_ssh_access_controller.rb`.
- Models: `app/models/org_ssh_key.rb`, `app/models/coolify_project_ssh_key.rb`,
  `app/models/coolify_project.rb`.
- Services: `app/services/coolify/host/ssh_source.rb`,
  `app/services/coolify/project/ssh_key_installer.rb`.
