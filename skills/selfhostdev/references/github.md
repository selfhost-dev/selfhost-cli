# GitHub integration

Connect GitHub accounts to an organization, list the repos an installation can
reach, list a repository's branches, and scan a repo for env vars and build
settings. Every `selfhost github ...` verb — `detect`, `connect`, `disconnect`,
`installations list`, `branches list` — answers
`not implemented yet: github ...` today, so use `selfhost api` with the paths
below. The resource is the GitHub App installation: an `ghi_<ulid>` pid scoped
to one organization and 1:1 with a GitHub account
(`app/models/github_app_installation.rb:6,29`). It is created by the browser
OAuth flow, not by an API call. All `/github_installations*` routes are
organization-scoped except `connected_accounts`, which spans every organization
the signed-in user belongs to.

## Before you start

- `selfhost auth status` must pass; the platform controller runs
  `authenticate_user!` on every action
  (`app/controllers/api/v1/platform/github_installations_controller.rb:8`).
  Below, `…:<line>` continues that file.
- Resolve an organization: `selfhost org use <slug>` or `--org <slug|pid>`. The
  controller looks the org up with `Organization.find_by(pid: params[:organization_id])`
  (`…:523`), so the query parameter must be an organization **pid** — the CLI
  resolves a slug to its pid and injects `organization_id` for you
  ([api.md](api.md#the-org-placeholder)).
- There is no installation to test against until someone runs the OAuth flow;
  public-repo endpoints work without one.
- Calling rules that matter here: a GET with parameters needs `-X GET` (a bare
  `-f` would send a POST); `-f KEY=VALUE` sends raw strings; `selfhost api`
  never prompts and refuses `--dry-run`. Read
  [api.md](api.md#method) and [commands.md](commands.md).
- Response bodies arrive as the envelope's `data` field only. Every endpoint
  below is read-only except `disconnect` and `destroy`.

## Endpoint map

### Installations

| Future CLI command | Method and path | What it does | Key parameters | Source |
| --- | --- | --- | --- | --- |
| `selfhost github installations list` | `GET /api/v1/platform/github_installations` | Installations of the org with `status != "removed"`, newest first, each with its first 100 repos | `organization_id` (pid, injected) | `config/routes.rb:111`, `…installations_controller.rb:20-36` |
| — (no verb) | `GET /api/v1/platform/github_installations/connected_accounts` | Every installation across all orgs you belong to, deduped by `installation_id` | none (org not required) | `config/routes.rb:118`, `…:51-77` |
| `selfhost github disconnect` | `DELETE /api/v1/platform/github_installations/disconnect` | Marks **all** the org's installations removed, then best-effort uninstalls on GitHub | `organization_id` | `config/routes.rb:114`, `…:257-296` |
| — | `DELETE /api/v1/platform/github_installations/:id` | Marks one installation removed, then best-effort uninstalls on GitHub | `id` = installation pid | `config/routes.rb:111`, `…:300-320` |

`disconnect` and `destroy` also require the `projects:destroy` platform role
(`…:11`, `app/controllers/concerns/platform_role_gate.rb:17`).

Response fields — list (`…:28-29`):

| Field | Notes |
| --- | --- |
| `pid` | installation pid `ghi_…` |
| `installation_id` | GitHub's numeric installation id |
| `account_login`, `account_type` | GitHub account (`User` or `Organization`) |
| `status` | `active` or `suspended`; rows marked `removed` by a disconnect no longer appear in either listing |
| `installed_at` | timestamp |
| `repos` | `name`, `full_name`, `url`, `private`, `default_branch`, `description` (`…:457-469`) |
| `repository_count` | GitHub's `total_count`, accurate even though `repos` is truncated |

`connected_accounts` returns the same fields minus `repos` and in a flat list
(`…:62-70`). Disconnect returns
`{disconnected_count, github_uninstalled_count, github_revoke_urls}`
(`…:290-293`); `destroy` returns `{github_uninstalled, github_revoke_url}`
(`…:315-317`).

### Branches

| Future CLI command | Method and path | What it does | Key parameters | Source |
| --- | --- | --- | --- | --- |
| `selfhost github branches list` | `GET /api/v1/platform/github_installations/:id/branches` | Branches of one repo through that installation — private repos work | `id` = installation pid, `repo_full_name`, `page`, `per_page` | `config/routes.rb:115`, `…:90-141` |
| `selfhost github branches list` | `GET /api/v1/platform/github_installations/branches` | Same, without an installation; **public repos only** | `repo_full_name`, `page`, `per_page`, `organization_id` | `config/routes.rb:117`, `…:90-141` |

- `repo_full_name` is required and must be `owner/repo`; a missing or malformed
  value is 422 (`…:416-429`).
- `page` defaults to 1 (minimum 1, no upper bound); `per_page` defaults to 100, clamped to 1..100 (`…:94-95`).
- Response: `{branches: [{name, sha, protected}], page, per_page}`, where `sha`
  is the branch head commit (`…:123-137`).

### Repo detection

| Future CLI command | Method and path | What it does | Key parameters | Source |
| --- | --- | --- | --- | --- |
| `selfhost github detect` | `POST /api/v1/platform/github_installations/detect_repo_config` | Scans a repo for env-var hints and build config; cached 5 min | `repo_url` (required), `installation_pid` (private repos), `branch` | `config/routes.rb:113`, `…:153-250` |

`repo_url` is the scannable field name, not `repo`: send a URL or shorthand
(`https://github.com/owner/repo`, `github.com/owner/repo`, `owner/repo`,
`git@github.com:owner/repo`) — it is normalized before the host check, and a
non-GitHub URL is 422 (`app/services/github/repo_url_normalizer.rb:19-33`,
`…:499-505`).

| Response field | Notes |
| --- | --- |
| `repo_url`, `repo_full_name`, `branch`, `private` | canonical values actually used |
| `detected_vars` | `[{key, hint, source}]`, sorted by key |
| `sources` | `{env_file, workflows, ci_files, docker, framework_files}` |
| `build_pack`, `port`, `dockerfile_path`, `base_directories`, `auto_build` | build-config service output |

`detected_vars`/`sources` shape: `app/services/github/repo_env_var_scanner.rb:74-90`;
build keys: `app/services/github/repo_build_config_service.rb:20-61,510-511`.

### OAuth (browser, not an API call)

| Future CLI command | Method and path | What it does | Key parameters | Source |
| --- | --- | --- | --- | --- |
| `selfhost github connect` | `GET /github/connect` | Returns `{redirect_url}` — the GitHub App install page, with a one-time state token | `organization_pid` (pid, required), `redirect_to_frontend` (optional) | `config/routes.rb:19`, `app/controllers/github/oauth_controller.rb:29-52` |
| — | `GET /github/callback` | GitHub redirects the user's **browser** here after install; the controller persists the installation and 302s to the console, or to the `redirect_to_frontend` address when one was passed to `connect` | `installation_id`, `setup_action`, `state` | `config/routes.rb:20`, `oauth_controller.rb:60-91` |

The callback is a browser redirect, not a JSON endpoint: it answers with a 302
to `$FRONTEND_URL/settings/github?status=connected|disconnected|error` — or to
the custom address passed as `redirect_to_frontend` to `connect`, with the same
status and message attached — and `status=error` carries one of
`invalid_state`, `installation_failed`, `unknown_action`,
`installation_request_pending`, `no_installation_found`
(`oauth_controller.rb:74-90,130-146,213-231`). The state token is an opaque
random value written to the server cache with a 10-minute TTL and deleted on
first use (`oauth_controller.rb:25,98-117`). The callback skips sign-in and
trusts only that one-time state token, so any browser carrying it completes the
flow: `selfhost api` still cannot complete it — it does not open a browser and
has no way to run GitHub's install screen — so `github connect` stays a console
action until the typed command lands.

## Connect and inspect one end to end

1. Check the session and pick the org — every `/api/v1/platform/github_installations*`
   path below inherits the injected `organization_id`.

   ```sh
   selfhost auth status
   selfhost org use acme
   ```

2. Start the OAuth flow and open the returned URL in a browser. The state token
   is generated server-side and is good for 10 minutes. `GET /github/connect`
   needs `organization_pid`, which the CLI does not fill in for you — the
   `{org}` placeholder is replaced only inside the path, never inside `-f`
   values, and the automatic value would be `organization_id` anyway — so write
   the organization pid out in full.

   ```sh
   selfhost api -X GET /github/connect -f organization_pid=org_abc123 -o json
   # data.redirect_url → https://github.com/apps/<slug>/installations/new?state=…&redirect_uri=…
   ```

3. Install the app on the GitHub account, then confirm what landed. `index`
   fetches up to 100 repos per installation live from GitHub, so it can take a
   moment and it fails soft: a GitHub outage yields `repos: []`,
   `repository_count: 0` for that installation
   (`…:436-455`).

   ```sh
   selfhost api /api/v1/platform/github_installations -o json
   ```

   Keep `data.installations[].pid` (`ghi_…`) — it is the `:id` for the member
   route and the `installation_pid` for detection.

4. Detect build settings for the repo. A public repo needs only `repo_url`;
   add `installation_pid` for a private one.

   ```sh
   selfhost api /api/v1/platform/github_installations/detect_repo_config \
     -f repo_url=https://github.com/acme/web -f branch=main -o json

   # private repo — installation_pid is required, and must be an active
   # installation of the org, otherwise 404 "GitHub installation not found"
   selfhost api /api/v1/platform/github_installations/detect_repo_config \
     -f repo_url=https://github.com/acme/private-web \
     -f installation_pid=ghi_01hzz… -f branch=main -o json
   ```

5. List the branches you can deploy. Through the installation first (works for
   private repos), then the public route as a fallback.

   ```sh
   selfhost api -X GET /api/v1/platform/github_installations/ghi_01hzz…/branches \
     -f repo_full_name=acme/private-web -f page=1 -f per_page=100 -o json

   # no installation: public repos only, org still required for the fallback token
   selfhost api -X GET /api/v1/platform/github_installations/branches \
     -f repo_full_name=acme/web -o json
   ```

6. Tear down. Both calls answer 422 while deployments are still linked — delete
   those deployments first (see Gotchas); the per-installation delete is second.

   ```sh
   # destructive: every installation of the org (refused while deployments are linked)
   selfhost api -X DELETE /api/v1/platform/github_installations/disconnect

   # destructive: one installation
   selfhost api -X DELETE /api/v1/platform/github_installations/ghi_01hzz…
   ```

## Day-2 operations

| Operation | Call |
| --- | --- |
| Audit every account you can use | `selfhost api /api/v1/platform/github_installations/connected_accounts` (no org needed) |
| Refresh a stale listing | Re-run `GET /api/v1/platform/github_installations`; repos are fetched live, nothing is stored |
| Re-check a repo after a push | `detect_repo_config` again — 5-minute cache per `repo/branch/installation`, branch resolution 10 minutes |
| Move an installation to another org | Not an endpoint: re-run the OAuth flow for the other org; GitHub reuses the same `installation_id` and the callback reassigns the row (`oauth_controller.rb:172-189`) |
| Revoke by hand | Use `github_revoke_urls` / `github_revoke_url` from the disconnect response when the GitHub uninstall failed |

## Gotchas

- **Public vs installed access.** The collection `branches` route serves public
  repos only. It resolves a token first and refuses a private repo outright
  (403, "List branches through a connected GitHub App installation"),
  deliberately: the fallback credential can be the platform's own and a tenant
  must never read platform-visible private repos through it
  (`…:90-104`, `…:331-363`).
- **Missing installation.** A repo the platform cannot see at all is 404 from
  the same route; GitHub rate limiting on the lookup is flattened to 422 with a
  "rate limit exceeded" message (`…:345-360`).
- **The public route still needs an org.** `set_organization` runs for every
  action except `connected_accounts`; with no `organization_id` the call is 404
  "Organization not found" (`…:9,523-525`).
- **`organization_id` is a pid, not a slug.** `Organization.find_by(pid: …)`
  is the only lookup; pass `--org <slug>` and let the CLI resolve it.
- **Token scope.** Requests are made with the *App installation* access token,
  cached 55 minutes (`app/services/github/app_auth_service.rb:37-58`). There is
  no user OAuth scope here, so a repo missing from an installation's grant
  cannot be reached by widening anything client-side — fix it in GitHub's
  installation settings and reconnect.
- **Public repos ignore `installation_pid` in detection.** The controller only
  consults `installation_pid` when the repo is private; passing it for a public
  repo is a silent no-op (`…:176-198`).
- **A 404 on token exchange means reconnect.** A removed/uninstalled
  installation makes `installation_access_token` raise, and the member
  `branches` route surfaces that as 422 "Could not list branches: …"
  (`app/services/github/app_auth_service.rb:47-53`, `…:139-140`).
- **Disconnect refuses while deployments exist.** Both `disconnect` and
  `destroy` render 422 listing the linked `repo_full_name`s — delete those
  deployments first (`…:400-412`).
- **`disconnect` also matches suspended rows.** It filters
  `where.not(status: "removed")`, so it removes suspended installations too,
  and it is not reversible from the CLI: re-running the OAuth flow is the way
  back (`…:258-273`, `oauth_controller.rb:130-145`).
- **GitHub-side removal is best-effort.** `github_uninstalled_count` can be
  lower than `disconnected_count`, or `github_uninstalled` false, while the
  platform side is already severed; the revoke URL is the manual fallback
  (`…:281-286`, `app/services/github/app_auth_service.rb:101-113`).
- **`connected_accounts` leaks across orgs by design.** It is user-scoped, not
  org-scoped, and dedupes on `installation_id`, so an installation transferred
  to another org appears once (`…:54-58`).
- **The callback carries no credentials.** `callback` skips
  `authenticate_user!`; the `state` token is what binds it to your org, is
  one-time, and expires in 10 minutes (`oauth_controller.rb:20-25,111-117`).

## Sources

- `config/routes.rb:18-21` (OAuth namespace), `config/routes.rb:111-119`
  (installations).
- `app/controllers/api/v1/platform/github_installations_controller.rb:8-14`
  (guards), `:20-36` (index), `:51-77` (connected_accounts), `:90-141`
  (branches), `:153-250` (detect_repo_config), `:257-296` (disconnect),
  `:300-320` (destroy), `:331-363` (public repo gate), `:368-396` (tokens),
  `:400-412` (deployment guard), `:416-501` (parsing), `:507-528` (scoping).
- `app/controllers/github/oauth_controller.rb:24-52` (connect),
  `:60-95` (callback), `:98-127` (state), `:130-205` (persistence),
  `:213-231` (frontend redirects).
- `app/models/github_app_installation.rb:6-31`;
  `app/services/github/app_auth_service.rb:37,61,85,101`;
  `app/services/github/repo_env_var_scanner.rb:74-90`;
  `app/services/github/repo_build_config_service.rb:20-61,510-511`;
  `app/services/github/repo_url_normalizer.rb:19-33`.
