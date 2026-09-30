# Catalog: regions, instance types, storage and pricing

`selfhost catalog regions|instance-types|storage-types|estimate` all answer
`not implemented yet: catalog <verb>` (exit 1), so choosing a region, an instance type, a storage type
and a price means reading the reference data with `selfhost api` and the paths below. Everything here
is read-only and comes from `config/data/*.json` plus the seeded `skus` rows — nothing is org-scoped,
and the single POST only computes a quote — except the Hetzner catalogue, which is read live
from the Hetzner API and cached briefly, with prices joined from the seeded `skus` rows.
The platform-side Hetzner catalogue (`GET /api/v1/platform/server_types`) answers the same question for Hetzner projects. Per-instance
PostgreSQL extension state (enable, list on a live instance) lives in [`postgresql.md`](postgresql.md);
the platform-wide extension catalogue is here.

## Before you start

- `selfhost auth status` — signed in. `/aws/v1/regions`, `/aws/v1/storage_types` and `/aws/v1/pricing`
  sit in the Firebase auth skip list (`config/application.rb:40`), so they answer even with an expired
  token; `/aws/v1/postgres_extensions` and `/api/v1/platform/server_types` do not.
- No `{org}` in any path here. When an organization is resolved the CLI still appends
  `organization_id` (query, or body on the POST); these controllers ignore it.
- Rules from [`api.md`](api.md) that matter here: a GET that carries parameters needs `-X GET`, or
  `-f`/`-F` turns it into a POST no route matches. `-F` types `true`/`false`/numbers, `-f` always
  sends text. One request per invocation — no pagination, no polling.

```sh
# read-only: every AWS region the platform prices
selfhost api /aws/v1/regions -o json

# read-only: 4+ vCPU, 16 GB+, current generation, in Frankfurt
selfhost api -X GET /aws/v1/regions/eu-central-1/instance_types \
  -F min_vcpu=4 -F min_memory=16 -F current_gen=true

# read-only: storage types with Frankfurt's rates merged in
selfhost api -X GET /aws/v1/storage_types -f region=eu-central-1
```

## Endpoint map

| Future CLI command | Method and path | What it does | Key parameters | Source |
|---|---|---|---|---|
| `selfhost catalog regions` | `GET /aws/v1/regions` | Every AWS region as `{code, display_name}` | none | `config/routes.rb:435`, `app/controllers/aws/v1/regions_controller.rb:21` |
| `selfhost catalog instance-types` | `GET /aws/v1/regions/:region_code/instance_types` | Types priced in the region, ascending by `hourly_price` | path `region_code`; `family`, `current_gen`, `min_vcpu`, `min_memory` | `config/routes.rb:436`, `regions_controller.rb:28`, `:44` |
| `selfhost catalog instance-types --provider hetzner` | `GET /api/v1/platform/server_types` | Hetzner locations × server types (≥ 4 GB RAM) | none | `config/routes.rb:32`, `app/controllers/api/v1/platform/server_types_controller.rb:16` |
| `selfhost catalog storage-types` | `GET /aws/v1/storage_types` | EBS volume types, region rates merged in | `region` | `config/routes.rb:438`, `app/controllers/aws/v1/storage_types_controller.rb:13` |
| — (no verb) | `GET /aws/v1/pricing` | Instance-type price list for one region, plus what the monthly price includes | `region` (default `us-east-1`) | `config/routes.rb:444`, `app/controllers/aws/v1/pricing_controller.rb:13` |
| `selfhost catalog estimate` | `POST /aws/v1/pricing/estimate` | Monthly quote for one shape; creates nothing | body keys below | `config/routes.rb:445`, `pricing_controller.rb:22` |
| `selfhost postgres extensions catalog` | `GET /aws/v1/postgres_extensions` | Platform-wide PostgreSQL extension catalogue; written up in [`postgresql.md`](postgresql.md) | `db_version`, `architecture` | `config/routes.rb:443`, `app/controllers/aws/v1/postgres_extensions_controller.rb:34` |

All seven are read-only; none bills, none mutates.

## Response fields

**`GET /aws/v1/regions`** — `data.regions[]`: `code`, `display_name`
(`config/data/aws_regions.json`, 35 regions, every one of them priced).

**`GET /aws/v1/regions/:region_code/instance_types`** — `data.region`, `data.total`,
`data.instance_types[]`: `instance_type`, `vcpu`, `memory_gib`, `storage`, `network_performance`,
`physical_processor`, `instance_family`, `is_current_generation`, `hourly_price` (USD/hour — the raw
AWS price, `app/services/cloud_provider/aws/aws_reference_data_service.rb:93`).

**`GET /aws/v1/storage_types`** — `data.storage_types[]`: `id`, `name`, `category`, `description`,
`price_per_gb_month`, `min_iops`, `max_iops`, `min_throughput_mbps`, `max_throughput_mbps`,
`iops_configurable`, `throughput_configurable`, `price_per_iops`, `price_per_mbps_throughput`,
`free_iops`, `free_throughput_mbps`, `min_storage_gb`, `max_storage_gb`
(`config/data/aws_storage_types.json`; six ids: `gp3`, `gp2`, `io1`, `io2`, `sc1`, `st1`). With
`region`, the regional `per_gb`/`per_iops`/`per_throughput` override the defaults and
`data.object_storage` (`id`, `name`, `price_per_gb_month`, `billed_on`, `requests_billed`) is added
for the ClickHouse S3 modes (`storage_types_controller.rb:33`, `:35`).

**`GET /api/v1/platform/server_types`** — body is `{status, data: {locations: [...]}}`; each location
carries `code`, `city`, `country`, `network_zone`, `server_types[]` with `code`, `name`, `cores`,
`memory_gb`, `disk_gb`, `cpu_type`, `architecture`, `category`, `offered`, `deprecated`,
`deprecation`, `available`, `hourly_price_cents`, `display_price`
(`app/services/cloud_provider/hetzner/reference_data_service.rb:206`).

**`GET /aws/v1/pricing`** — `data.currency`, `effective_region`, `supported_regions[]` (`code`,
`display_name`), `monthly_pricing_basis`, `included_in_monthly_price[]`, `billed_separately[]`,
`instance_types[]` (`instance_type`, `hourly_price`, `monthly_price` = raw price × 730)
(`app/services/pricing/hybrid_pricing_service.rb:143`).

## Choose a region, a type and a storage type

1. Region — `GET /aws/v1/regions`, then keep the ones `GET /aws/v1/pricing` lists in
   `supported_regions` (region code is the same string the estimate takes). `us-east-1` and
   `eu-central-1` are the usual picks; both are priced.

```sh
selfhost api /aws/v1/regions -o json
selfhost api -X GET /aws/v1/pricing -f region=eu-central-1 -o json
```

2. Instance type — from the region's list. `family` matches as a case-insensitive substring of the
   display family ("General purpose", "Compute optimized", "Memory optimized", …), `min_vcpu` and
   `min_memory` compare against `vcpu` and `memory_gib` (`aws_reference_data_service.rb:120`), and
   the list already excludes types EC2 does not offer in the region
   (`app/services/cloud_provider/aws/region_instance_type_offerings.rb:41`).

```sh
selfhost api -X GET /aws/v1/regions/eu-central-1/instance_types \
  -F min_vcpu=4 -F min_memory=16 -F current_gen=true -o json
```

The row you get for `m7g.xlarge` (4 vCPU, 16 GiB, Graviton3):

```json
{"instance_type":"m7g.xlarge","vcpu":4,"memory_gib":16.0,"storage":"EBS only",
 "network_performance":"Up to 12500 Megabit","physical_processor":"AWS Graviton3 Processor",
 "instance_family":"General purpose","is_current_generation":true,"hourly_price":0.1955}
```

3. Storage type — `gp3` for the data volume unless the workload needs provisioned IOPS (`io2`) or a
   bulk tier (`sc1`/`st1`). Read the type's `min_storage_gb`/`max_storage_gb` and its
   `price_per_iops`/`price_per_mbps_throughput` here; both rates are charged only above the
   `free_iops`/`free_throughput_mbps` baseline.

```sh
selfhost api -X GET /aws/v1/storage_types -f region=eu-central-1 -o json
```

## Estimate the monthly cost

`POST /aws/v1/pricing/estimate` takes a flat JSON object. Keys, exactly as
`pricing_controller.rb:35` permits them (`app/services/pricing/hybrid_pricing_service.rb:22`):

| Body key | Required | Accepted values | Meaning |
|---|---|---|---|
| `region` | yes | region code from `aws_regions.json` | any valid region; the shape must be priced there |
| `instance_type` | yes | key of `aws_instance_specs.json` | compute for the DB unit |
| `storage_type` | yes | an id in the region's storage pricing (`gp3`, `gp2`, `io1`, `io2`, `sc1`, `st1`) | data-volume type |
| `storage_size_gb` | yes | ≥ 1 and within the type's `min_storage_gb`..`max_storage_gb` | data-volume size |
| `database_count` | yes in practice | ≥ 1 (the default, 0, is rejected) | primaries |
| `replica_count` | no (0) | ≥ 0 | replicas; priced exactly like a primary |
| `public_ipv4_count` | no (0) | ≥ 0 | each IP is priced per month |
| `mode` | no (`managed`) | `managed`, `byoc` | `byoc` switches to the per-database shape below |
| `db_type` | no | engine name, e.g. `postgresql` | probe's engine; selects an engine-wide margin override |
| `iops` | no | type's `min_iops`..`max_iops` when `iops_configurable` | defaults to `free_iops` (gp3: 3000) |
| `throughput_mbps` | no | type's `min_throughput_mbps`..`max_throughput_mbps` when `throughput_configurable` | defaults to `free_throughput_mbps` (gp3: 125) |

A bad value is a 422 with the service's own message (exit 1): invalid region/mode/type
(`hybrid_pricing_service.rb:209`, `:214`, `:226`), size/IOPS/throughput out of range (`:232`, `:239`,
`:248`), or SKU rows missing for the shape (`:316`).

```sh
# read-only: 1 primary + 1 replica, PostgreSQL, 100 GB gp3 in Frankfurt
selfhost api /aws/v1/pricing/estimate \
  -f region=eu-central-1 -f instance_type=m7g.xlarge -f storage_type=gp3 \
  -F storage_size_gb=100 -F database_count=1 -F replica_count=1 -f db_type=postgresql -o json
```

`total_monthly` is compute + both volumes + IPv4, for `database_count + replica_count` units:

```json
{"currency":"USD","pricing_model":"regional_cost_plus_markup","mode":"managed",
 "region":"eu-central-1","instance_type":"m7g.xlarge","storage_type":"gp3",
 "storage_size_gb":100.0,"database_count":1,"replica_count":1,"public_ipv4_count":0,
 "iops":3000.0,"throughput_mbps":125.0,
 "unit_prices":{"instance_hourly":0.2639,"instance_monthly":192.67,"storage_monthly":15.42,
                "public_ipv4_monthly":4.9275},
 "breakdown":{"compute_base_monthly":285.43,"storage_base_monthly":22.85,
              "markup_monthly":107.91,"public_ipv4_monthly":0.0},
 "total_monthly":416.19}
```

`unit_prices` are per database unit per month; `breakdown` splits the pre-margin cost from the markup.
The numbers above follow from the seed data — 0.1955 USD/h compute at the seeded 35 % margin
(`db/seeds/skus.rb:97`), gp3 at 0.0952 USD/GB-month, a 100 GB data volume plus the 20 GB gp3 boot
volume (`app/services/billing/aws_storage_cost_service.rb:8`,
`app/services/cloud_provider/aws/ec2_provisioning_service.rb:12`), two units, no extra IOPS or
throughput.
Add `-F public_ipv4_count=1` for `+4.9275`; raise `iops`/`throughput_mbps` above the free baseline to
see the extra lines move. With `-f mode=byoc` the response changes shape entirely:
`pricing_model: "byoc_per_database"`, with `unit_prices` holding `instance_hourly`,
`managed_db_monthly` and `byoc_per_db_monthly`, and `breakdown` holding `managed_equivalent`,
`byoc_rate`, `per_db_monthly` and `total_database_units` (`hybrid_pricing_service.rb:66`).

## Extensions catalogue (no instance)

`GET /aws/v1/postgres_extensions` is the one catalogue that must answer *before* a database exists —
the create form, and Coolify databases, which take extensions only at creation. It is the same
platform capability the instance form needs, so it is documented with the rest of the extension
flow: see [`postgresql.md`](postgresql.md). Its shape is different from the AWS reference data here —
`selfhost api -X GET /aws/v1/postgres_extensions -f db_version=17 -f architecture=arm64` returns
`available` / `unavailable` rows rather than a price list, and — like `/api/v1/platform/server_types`,
unlike the AWS reference endpoints above — it is not in the auth skip list
(`config/application.rb:40`).

## Gotchas

- **Two rates for the same thing (AWS).** Every AWS figure in this file is the *unmargined* reference
  price: `hourly_price` on instance types, `price_per_gb_month` on storage types. Billing prices from
  the `skus` rows, which carry the seeded 35 % margin (50 % for ClickHouse, `db/seeds/skus.rb:97`,
  `:339`). `GET /aws/v1/pricing`'s `instance_types[].monthly_price` is likewise raw. Use the estimate
  for what a customer actually pays.
- **The boot volume is always in the quote.** `storage_monthly` is the data volume *plus* a 20 GB gp3
  boot volume at the region's gp3 rate, whatever storage type you picked
  (`app/services/pricing/hybrid_pricing_service.rb:379`).
- **The quote is organization-independent.** The probe instance is not persisted and carries no
  organization, so `MarginResolver` falls through to the engine-wide `(nil, db_type)` override and
  then the SKU's percentage; a per-organization margin override is not reflected
  (`app/services/billing/margin_resolver.rb:57`).
- **`current_gen` filters on presence, not truth.** Any non-empty `current_gen` — including `false` —
  selects current-generation types only; you cannot ask for the old ones
  (`aws_reference_data_service.rb:122`).
- **`GET /aws/v1/regions*` is rate-limited to 2 requests/second per client IP** — both actions share
  it (`regions_controller.rb:17`), so looping over 35 regions trips it. The 429 carries no
  `Retry-After`; `api` retries once and then exits 75. Everything else here (storage types, pricing,
  extensions, server types) allows 60 requests/minute.
- **Caching is upstream, not just a fast read.** `config/data` is memoized in-process for the life of
  the worker (`aws_reference_data_service.rb:13`; `reload!` exists but no endpoint calls it), EC2
  instance-type offerings are cached 24 h with a 10-minute negative cache
  (`region_instance_type_offerings.rb:10`, `:21`), and Hetzner locations/server types are cached 15
  minutes/60 seconds (`cloud_provider/hetzner/reference_data_service.rb:81`, `:83`). A price or
  availability change upstream shows up late.
- **Fail-open on offerings.** When the EC2 offerings lookup fails for a region, the instance-type
  filter is skipped rather than emptied, so a region can list types AWS will not actually launch
  (`region_instance_type_offerings.rb:41`).
- **Different units per provider.** AWS `hourly_price` is USD/hour; Hetzner `hourly_price_cents` is
  margined cents/hour next to a formatted `display_price`. `available: null` on a Hetzner entry means
  "not sold in this location", not "out of stock" (`reference_data_service.rb:184`).
- **`GET /api/v1/platform/server_types` supports ETags** and answers `304` when you send a matching
  `If-None-Match`; the CLI sends none unless you add it yourself with `-H`
  (`server_types_controller.rb:30`). The response key is `locations`, not `server_types`
  (`server_types_controller.rb:30`).
- **Not in the estimate:** block-storage snapshots, S3 object storage (ClickHouse
  `object_storage`/`tiered` bills per GiB-month on top), backups and egress. `billed_separately` on
  `GET /aws/v1/pricing` lists what the per-instance monthly price excludes.
- **Nothing here is org-scoped**, so `--org` changes nothing except the injected `organization_id`;
  the POST body accepts that extra key silently.

## Sources

- `config/routes.rb:29-32` (`/api/v1/platform/server_types`), `:326-327` (`namespace :aws` →
  `/aws/v1`), `:435-436` (regions + member `instance_types`), `:438`, `:443`, `:444`, `:445`.
- `app/controllers/aws/v1/regions_controller.rb`, `storage_types_controller.rb`, `pricing_controller.rb`,
  `postgres_extensions_controller.rb`; `app/controllers/api/v1/platform/server_types_controller.rb`;
  `app/controllers/concerns/response_handler.rb:7` (envelope, `:92` for the 429 body).
- `app/services/cloud_provider/aws/aws_reference_data_service.rb`, `.../region_instance_type_offerings.rb`,
  `app/services/cloud_provider/hetzner/reference_data_service.rb`,
  `app/services/pricing/hybrid_pricing_service.rb`, `app/services/postgres_extensions/catalog.rb`,
  `app/services/billing/storage_cost_components.rb`, `app/services/billing/aws_storage_cost_service.rb`,
  `app/services/billing/rate_math.rb`, `app/services/billing/margin_resolver.rb`,
  `app/services/billing/component_registry.rb`.
- Data: `config/data/aws_regions.json`, `aws_instance_specs.json`, `aws_region_pricing.json`,
  `aws_storage_types.json`, `aws_region_storage_pricing.json`, `aws_region_s3_pricing.json`;
  `db/seeds/skus.rb`. CLI surface: `src/cli/catalog.rs`.
