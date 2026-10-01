# Raw API access: `selfhost api`

`selfhost api` sends one HTTP request to any platform endpoint with the selected
profile's base URL and credentials, the CLI's organization handling, output
formats and exit codes. It is the escape hatch for endpoints that have no typed
command yet.

Everything below is verified against `selfhost api --help`, `src/cli/api.rs` and
`src/api/mod.rs` at commit `4739a1e`.

The global flags work as everywhere else: `--profile`, `--base-url`, `--org`,
`-o`/`--format`, `--json`, `--timeout` (one request, default 30 s), `-v`,
`--debug`, `-q`. `api` never prompts, so `-y`/`--yes` and `--poll-interval` do
nothing here.

## When `api` is the right tool

Use it when:

- the typed command does not exist yet. `project`, `deploy`, `github`, `domain`,
  `postgres`, `mysql`, `redis`, `clickhouse`, `opensearch`, `catalog`,
  `billing`, `cloud`, `network`, `ssh-key`, `alert`, `scaling`, `webhook` and
  `config` answer `not implemented yet: <group> <verb>` for every verb;
- the endpoint has no verb of its own at all;
- you need a request shape no typed command exposes.

Prefer the typed command when one exists (`auth`, `profile`, `org`; local only:
`tui`, `tree`, `completion`, `help`). Typed commands validate fields, confirm
destructive or money-affecting work, refuse `--dry-run` (nothing previews yet)
and print curated tables. `api` does none of that: it sends exactly what you
pass and prints what the server returns, in a single request — no pagination
loop, no polling.

## Endpoint argument

The one positional argument is a path, with or without a leading slash
(`organizations` becomes `/organizations`). Case and any `?query` are kept:
`/organizations?page=2` is sent as written.

Rejected with exit 2 (usage error, before any credential is read and before any
request is sent):

| Input | Message |
| --- | --- |
| empty string | `endpoint is empty; pass a path such as /organizations` |
| contains `://` | `endpoint must be a path, not a full URL; pass a path such as /organizations` |
| whitespace or control character anywhere | `endpoint must not contain whitespace or control characters` |
| `..` as a whole path segment | `endpoint must not contain .. path segments` |

`..` in the query is fine (`/search?q=..` passes). A literal space must be
percent-encoded.

Query merging: a `?query` on the endpoint is preserved, and parameters passed
with `-f`/`-F` are appended after it. The resolved organization's
`organization_id` is appended too (see below), unless the endpoint query or a
parameter already carries `organization_id`. Parameter values are
percent-encoded by the URL builder; a `?query` you wrote on the endpoint is
passed through as you wrote it.

## Method

- Default: `GET`.
- Adding any `-f`, `-F` or `--input` switches it to `POST`, unless `-X` names a
  method.
- `-X` accepts `GET`, `POST`, `PUT`, `PATCH`, `DELETE`, case-insensitively
  (`-X patch` works). Anything else — including an empty value — exits 2:
  `unknown method '<value>'; pass one of GET, POST, PUT, PATCH, DELETE`.
- `-X GET` is the escape hatch for parameters on a GET: it wins over `-f`/`-F`
  and keeps them on the query string. Without it, `selfhost api /organizations
  -f per_page=10` is a POST.

There is no `HEAD` or `OPTIONS`.

## Parameters

- `-f KEY=VALUE` (raw text): the value is always a string, and `@` is not
  special (`-f at=@-` sends the literal text `@-`).
- `-F KEY=VALUE` (typed): `true`, `false`, `null` and integers become JSON
  booleans, null and numbers; everything else stays a string (so `1.5` and
  `True` are strings).
- `-F KEY=@path` reads that file as UTF-8 text and inserts it as one string —
  including any trailing newline. `-F KEY=@-` reads all of stdin.
- Both flags are repeatable and merge in the order given (`-f` values first,
  then `-F`). A later duplicate key replaces an earlier one.
- `KEY` must be non-empty and the item must contain `=`; otherwise exit 2:
  `raw field '<item>' needs KEY=VALUE form` or `field '<item>' needs KEY=VALUE
  form`. `key[sub]` is a literal key — there is no nested-parameter syntax.
- An unreadable `@file` exits 1 (`cannot read file '<path>': …`); an unreadable
  `--input` file exits 1 (`cannot read input file '<path>': …`). Both fail
  before the sign-in check.

Where the values go:

| Invocation | Fields | Body |
| --- | --- | --- |
| GET, or `-X GET` | query string, typed values rendered (`true`, `null`, `42`) | none |
| write method, no `--input` | one JSON object body | — |
| any method with `--input FILE` or `--input -` | query string | the file or stdin bytes, sent as-is with `Content-Type: application/json` |

Organization injection: whenever an organization is resolved (`--org`,
`SELFHOSTDEV_ORG`, or the profile's saved organization), the request carries
`organization_id` — appended to the query for GET/`--input` requests, inserted
into the JSON object body for writes. Pass `organization_id` yourself (as a
parameter or on the endpoint query) to suppress it. There is no flag that turns
injection off, so raw calls are always scoped to your organization when one is
resolved.

## Headers

- `-H "Name: Value"`, repeatable. Split on the first colon, both halves trimmed:
  `-H "X-Token: a:b"` sends the value `a:b`.
- Blocked, case-insensitively: `authorization`, `accept`, `content-type`,
  `content-length`. Passing one fails the whole command (exit 2) with
  `header '<name>' is managed by the CLI and cannot be overridden` — it is never
  dropped or overridden.
- Missing colon or empty name: `header '<item>' needs NAME: VALUE form`; an
  invalid header name or value is also a usage error. All exit 2 before a
  request.
- The CLI always sends `Authorization: Bearer <token>` and
  `Accept: application/json` itself; write bodies get `Content-Type:
  application/json`.
- `-v` echoes `→ METHOD path` to stderr before sending (the query string is not
  echoed). `--debug` adds `Authorization: Bearer [REDACTED]` and one
  `Name: [set]` line per extra header — names only, never values — plus the
  request body redacted.
- `-i` prints the response headers (below).

## The `{org}` placeholder

- Only the literal `{org}` is replaced, with the resolved organization's **pid**
  (`org_…`), not the slug: `/organizations/{org}/members` →
  `/organizations/org_abc123/members`. No other placeholder exists — `{owner}`
  stays in the path and reaches the server verbatim.
- Resolution order: `--org <slug|pid>` or `SELFHOSTDEV_ORG` (a pid is used as
  is, anything else is looked up among your organizations with an extra
  request) wins over the organization saved in the profile
  (`selfhost org use <slug>`). An empty value counts as unset.
- With `{org}` in the path and no organization resolved, the command exits 2
  with `no organization selected; run selfhost org use <slug>` — before any
  credential is read and without sending anything.
- Without `{org}` the path is sent as written; you can still target one
  organization by writing its pid into the path or passing
  `-f organization_id=org_…`.

## Output

- Default: only the `data` field of the response envelope (the payload), not
  the envelope itself; a JSON object body with no `data` field prints `null`. A
  success body that is not the envelope shape (plain text, HTML, a bare array)
  is printed as raw text.
- `-o table|json|yaml` or `--json`: table is the default on a terminal, JSON
  when piped. Plain-text bodies keep control characters in JSON/YAML and have
  them stripped in table mode (JSON payloads are escaped by the serializer).
- `-i` / `--include` prints `HTTP <status>`, then the response headers sorted by
  lowercase name (original casing kept), a blank line, then the body rendered in
  the resolved format.
- `--silent` prints nothing on stdout and leaves the exit code to say how it
  went; failures still print their error on stderr.

## Errors and exit codes

| Result | Exit | stderr |
| --- | --- | --- |
| 2xx and envelope `status` is not `"error"` | 0 | — |
| any other status (400, 403, 404, 405, 409, 422, 5xx, …) | 1 | the envelope `message`, or `HTTP <code>: <first 160 characters of the body>` when there is none; the generic case also appends `(HTTP <code>)` |
| 401 | 3 | `not authenticated: <message>` — run `selfhost auth login` |
| 402 | 4 | `billing required: <message>` |
| 429 that survives the retry | 75 | `rate limited: <message>` |
| bad endpoint, method, field, header, `{org}` without an organization, `--dry-run` | 2 | `usage error: <message>` |
| unreadable input file, transport or TLS failure, or a request past `--timeout` | 1 | the underlying error message |

An error status is an error even when the HTTP status is 2xx but the body
carries `"status": "error"`. A 429 is slept off (its `Retry-After` in seconds,
capped at 60, default 1) and replayed exactly once; a second 429 exits 75.

Redaction rule: responses are printed as the server sent them. Some endpoints
return one-time secrets or tokens in `data`; a response body can also be printed
verbatim by `-i`. Never paste `api` output into logs, files, commits or a
conversation, and never re-echo credentials an endpoint returns. `--debug`
redacts the *request* only, not the response.

## Dry runs

`--dry-run` is refused for `api` in the shared gate, before any handler runs: exit
2, `dry runs are not supported for api calls yet; nothing was sent`. Every other
command refuses it too — see [commands.md](commands.md). There is no preview that
avoids sending; `-v`/`--debug` show the request but still send it.

## Not supported

Do not spend turns on these — `api` has no such flags, and an unknown flag is a
parse error (exit 2):

- `--jq`, `--jmespath` or output templates (`--template` exists only as
  `project service create`'s service-template flag, a stub anyway),
- `--paginate`, `--slurp` (one request per invocation; follow `page`/`per_page`
  parameters yourself),
- nested parameters such as `key[sub]` (literal key, not a nested object),
- `HEAD` and `OPTIONS`,
- any GraphQL awareness: no query builder, no variables handling, and the
  endpoint rejects whitespace, so a query would have to travel as a raw body,
- placeholders other than `{org}` — `{owner}` and friends are sent verbatim.

## Worked examples

All examples assume the CLI is on PATH and a profile is signed in.

```sh
# read-only: every organization you belong to, as JSON
selfhost api /organizations -o json

# read-only: one organization's details (pid substituted for {org})
selfhost api /organizations/{org} --org acme -o yaml

# read-only: page a list — -X GET keeps per_page on the query string
selfhost api -X GET /organizations -f per_page=10 -f page=2

# read-only: the people in one organization
selfhost api /organizations/{org}/members --org acme

# mutating: invite someone; the body is a flat JSON object of the -f pairs
selfhost api /organizations/{org}/invitations --org acme \
  -f email=dana@example.com -f role_pid=role_member
# mutating: JSON body from stdin, fields (none here) stay on the query.
# The update endpoint needs its fields nested under an organization key.
echo '{"organization":{"description":"Staging and demos"}}' \
  | selfhost api -X PATCH /organizations/{org} --org acme --input -

# read-only: on a GET every value travels on the query string, so -f and -F
# both send limit=10 here; -F typing only shapes JSON write bodies
selfhost api -X GET /organizations/{org}/activity_logs --org acme \
  -F page=2 -F limit=10

# read-only: -i adds the status line and response headers
selfhost api /organizations/{org} --org acme -i -o json

# read-only: in a script — nothing on stdout, the exit code is the signal
selfhost api /organizations --silent || { echo "api call failed ($?)" >&2; exit 1; }

# read-only: a custom header; -v shows the method that will actually be used
selfhost api /organizations -H 'X-Trace: cli-check' -v

# read-only: the trap — a bare -f would turn this into a POST:
#   selfhost api /organizations/{org}/activity_logs -f page=2 --org acme
# -X GET is what keeps the parameters on the query string:
selfhost api -X GET /organizations/{org}/activity_logs -f page=2 --org acme
```
