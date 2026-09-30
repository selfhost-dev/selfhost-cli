# Network

VPCs, subnets and security groups in the AWS account behind your organization. Every
`selfhost network ...` verb (`vpc`, `subnet`, `security-group` × `list|show|create|delete`)
answers `not implemented yet: network <...>` and exits 1, so the work goes through
`selfhost api` with the paths below. All three resources are org-scoped under
`/aws/v1/organizations/:organization_id/...`, and that segment is the organization **pid**
(`org_…`): each controller resolves it with `Organization.find_by(pid: params[:organization_id])`
plus a membership check (`app/controllers/aws/v1/vpcs_controller.rb:91-97`, mirrored in
`subnets_controller.rb:76-82` and `security_groups_controller.rb:228-234`). These resources have no
platform pid — you address them by their AWS ids (`vpc-…`, `subnet-…`, `sg-…`). A created VPC also
gets a `CloudVpc` row, which delete and the security-group ownership check depend on
(`app/models/cloud_vpc.rb:1-7`).

## Before you start

- `selfhost auth status` (exit 3 means not signed in); an organization resolved (`--org`,
  `SELFHOSTDEV_ORG` or `selfhost org use`) because every path carries `{org}`.
- The member's organization role must carry the permission for the action. Each action runs
  `authorize! :vpcs|:subnets|:security_groups, :read|:create|:delete|:update`
  (`app/controllers/aws/v1/vpcs_controller.rb:11-13`, `app/controllers/aws/v1/subnets_controller.rb:9-10`,
  `app/controllers/aws/v1/security_groups_controller.rb:11-13`); a role without the key gets 403
  `You do not have permission to perform this action.`
  (`app/controllers/concerns/authorization_concern.rb:50-51`, `:81-86`). Only the seeded **owner**
  role has it (`"all" => true`, `db/seeds.rb:11`); admin, manager, billing and member carry no
  `vpcs`/`subnets`/`security_groups` keys (`db/seeds.rb:14-71`), so a non-owner gets 403 on every
  command in this file — unless a platform admin is acting in an explicitly named
  organization, which passes the role check without a membership
  (`app/controllers/concerns/authorization_concern.rb:35-40`).
- Calling contract: [api.md](api.md). The rules that bite here:
  - a GET carrying parameters needs `-X GET`, otherwise `-f`/`-F` turns the call into a POST;
  - `-f` is always a string, `-F` types `true`/`false`/numbers, and `key[sub]` is a literal key —
    an array of rule objects can only travel as a JSON body via `--input -`;
  - `{org}` is replaced with the resolved org pid, and the resolved `organization_id` is appended
    to the query too (harmless: the route segment already carries the same pid).
- `cloud_credential_id` is optional on every endpoint here. Omit it to use platform-managed
  credentials, pass it to use a BYOC credential; one the org cannot see is 403
  (`app/controllers/concerns/aws_credential_resolver.rb:18-46`).
- `region` is a per-request parameter, never stored on the org. Subnet and security-group
  `index`/`create` reject a blank region with 422; VPC `index`/`create` do not check it.

## Endpoint map

`{org}` in the paths is the organization pid. Every row is reachable today via `selfhost api`;
the first column is the typed verb that will replace it.

### VPC

| Future CLI command | Method and path | Effect | Key parameters | Source |
|---|---|---|---|---|
| `network vpc list` | `GET /aws/v1/organizations/{org}/vpcs` | read-only; lists AWS VPCs in a region, each annotated `type: "public"`/`"private"` | `region`; `cloud_credential_id` optional | `config/routes.rb:388`, `app/controllers/aws/v1/vpcs_controller.rb:15-24` |
| `network vpc create` | `POST /aws/v1/organizations/{org}/vpcs` | mutating; creates a VPC, an internet gateway, a public main route, and up to 3 subnets | `region`, `cidr_block`; `vpc_name` optional | `config/routes.rb:388`, `app/controllers/aws/v1/vpcs_controller.rb:32-52` |
| `network vpc delete` | `DELETE /aws/v1/organizations/{org}/vpcs/:id` | destructive; deletes the VPC and everything in it | `id` = AWS `vpc-…` (path); no `region` | `config/routes.rb:388`, `app/controllers/aws/v1/vpcs_controller.rb:64-77` |
| `network vpc show` | — no route — | — | — | `resources :vpcs` declares `index`, `create`, `destroy` only |

### Subnet

| Future CLI command | Method and path | Effect | Key parameters | Source |
|---|---|---|---|---|
| `network subnet list` | `GET /aws/v1/organizations/{org}/subnets` | read-only; every subnet in the region | `region` (required) | `config/routes.rb:389`, `app/controllers/aws/v1/subnets_controller.rb:12-21` |
| `network subnet create` | `POST /aws/v1/organizations/{org}/subnets` | mutating; creates one subnet | `region`, `vpc_id`, `cidr_block`, `availability_zone` (required); `name` is a tag only | `config/routes.rb:389`, `app/controllers/aws/v1/subnets_controller.rb:32-63` |
| `network subnet show` / `network subnet delete` | — no route — | — | — | `resources :subnets` declares `index`, `create` only |

### Security group

| Future CLI command | Method and path | Effect | Key parameters | Source |
|---|---|---|---|---|
| `network security-group list` | `GET /aws/v1/organizations/{org}/security_groups` | read-only; every security group in the region | `region` (required) | `config/routes.rb:390`, `app/controllers/aws/v1/security_groups_controller.rb:15-26` |
| `network security-group create` | `POST /aws/v1/organizations/{org}/security_groups` | mutating; creates a group and its ingress rules, returns 201 | `region`, `group_name`, `description`, `vpc_id` (required); `ip_permissions` optional | `config/routes.rb:390`, `app/controllers/aws/v1/security_groups_controller.rb:40-76` |
| `network security-group update` | `PUT /aws/v1/organizations/{org}/security_groups` | mutating + destructive; **replaces the whole ingress set** | `region`, `security_group_id`, `ip_permissions` (required) | `config/routes.rb:387`, `app/controllers/aws/v1/security_groups_controller.rb:90-211` |
| `network security-group update` (by id) | `PATCH /aws/v1/organizations/{org}/security_groups/:id` | same action as the PUT above | `id` = AWS `sg-…` (path), `region`, `ip_permissions` | `config/routes.rb:390`, `app/controllers/aws/v1/security_groups_controller.rb:90-211` |
| `network security-group show` / `network security-group delete` | — no route — | — | — | `resources :security_groups` declares `index`, `create`, `update` only |

The two update routes run the identical action; the org-level `PUT` has no `:id` segment, so there the
group id must arrive as `security_group_id` in the body
(`app/controllers/aws/v1/security_groups_controller.rb:92`).

## Provision one end to end

Steps 1-4 build the network, step 5 puts an instance in it, step 6 tears it down. Replace
`vpc-0abc123`/`sg-0abc123` with the ids returned by the earlier steps, and pick a real region.

```sh
# 1. mutating: create the VPC (/16 in this example). data comes back as the CloudVpc row:
#    vpc_id, region, cidr_block, name, organization_id, user_id, is_platform_vpc, created_at
selfhost api /aws/v1/organizations/{org}/vpcs --org acme \
  -f region=eu-central-1 -f cidr_block=10.20.0.0/16 -f vpc_name=acme-app

# 2. read-only: see what the creator built (an IGW, a public main route, up to 3 subnets)
selfhost api -X GET /aws/v1/organizations/{org}/subnets --org acme -f region=eu-central-1
```

The creator derives its subnets from the VPC CIDR, not from you: for a `/16` it makes
`10.20.0.0/24`, `10.20.1.0/24`, `10.20.2.0/24` in the first three availability zones
(`app/services/cloud_provider/aws/vpc_creator_service.rb:95-116`). They carry only a
`managed-by` tag, so no `name`.

```sh
# 3. mutating: add your own subnet if you need a specific AZ/range
selfhost api /aws/v1/organizations/{org}/subnets --org acme \
  -f region=eu-central-1 -f vpc_id=vpc-0abc123 -f cidr_block=10.20.9.0/24 \
  -f availability_zone=eu-central-1a -f name=acme-app-a

# 4. mutating: a security group in that VPC. ip_permissions is an array of objects, so send a
#    JSON body -- the CLI has no nested-parameter syntax. Keys here are snake_case
#    (ip_protocol / from_port / to_port / ip_ranges), see Gotchas.
cat <<'JSON' | selfhost api /aws/v1/organizations/{org}/security_groups --org acme --input -
{"region":"eu-central-1","vpc_id":"vpc-0abc123","group_name":"acme-app",
 "description":"app ingress",
 "ip_permissions":[{"ip_protocol":"tcp","from_port":5432,"to_port":5432,"ip_ranges":["203.0.113.7/32"]}]}
JSON
```

Step 5 is **billable** (an EC2 instance is created). It goes through the instances endpoint, not
this domain: `POST /aws/v1/instances` (`config/routes.rb:328`) accepts `vpc_id` and places the node
there (`app/controllers/aws/v1/instances_controller.rb:742-752`, `:796-805`). What it ignores at
create is a `subnet_id` or `security_group_ids` — the platform picks subnets with
`SubnetSelectorService` and builds its own per-instance security group
(`app/services/cloud_provider/aws/security_group_service.rb:205-276`). So the subnets and security
group you made above are **not** attachable by id at create; only the VPC is. See the instances
reference for the rest of that request's required fields.

```sh
# 5. billable + mutating: put an instance in the VPC you built (instances domain).
#    storage is a nested object and key[sub] is a literal key to the CLI, so the body goes
#    as JSON via --input -. identifier is required (the request is rejected without it);
#    vpc_id is the field this domain contributes; the instances
#    reference has the full required-field list.
cat <<'JSON' | selfhost api /aws/v1/instances --org acme --input -
{"type_of_dbms":"postgres","identifier":"acme-app-db","region":"eu-central-1",
 "instance_type":"t3.medium","vpc_id":"vpc-0abc123","replica_count":1,
 "storage":{"type":"gp3","size":100,"iops":3000,"throughput":125}}
JSON
```

```sh
# 6. teardown, in dependency order: the instance first, then the VPC.
#    deleting the VPC also deletes its subnets, non-default NACLs, non-main route tables and
#    non-default security groups, and detaches+deletes its internet gateways.
selfhost api -X DELETE /aws/v1/organizations/{org}/vpcs/vpc-0abc123 --org acme
```

## Day-2 operations

Only the security group is mutable through this domain; VPCs and subnets have no update route.

| Operation | Call | Notes |
|---|---|---|
| Replace a security group's ingress rules | `PATCH /aws/v1/organizations/{org}/security_groups/:id` with `region` + `ip_permissions` | Revoke-all-then-authorize; whole rule set, not additions (`app/services/cloud_provider/aws/security_group_service.rb:137-163`) |
| Attach a security group list to an existing instance | `PATCH /aws/v1/instances/:id` (`config/routes.rb:336`) with `security_group_ids` | Runs `modify_instance_attribute(groups:)` and **replaces** the instance's whole SG list; async via `UpdateInstanceJob` (`app/services/cloud_provider/aws/instance_updaters/network_updater_service.rb:26-34`) |
| Change an existing instance's subnet | `PATCH /aws/v1/instances/:id` with `subnet_id` | Placeholder: it only logs a warning and changes nothing (`network_updater_service.rb:38-50`) |
| Revoke one port's rules | no endpoint | `revoke_port_ingress` exists but is only reached through instance updates |

## Gotchas

- **Security-group update is a full replace.** `update_inbound_rules` revokes every existing ingress
  rule, then authorizes the array you send (`app/services/cloud_provider/aws/security_group_service.rb:143-163`).
  Sending one rule deletes all the others. It does not add.
- **Two different key shapes for the same `ip_permissions` field.** `create` reads snake_case through
  the params object — `perm[:ip_protocol]`, `:from_port`, `:to_port`, `:ip_ranges` as an array of CIDR
  strings (`security_group_service.rb:78-81`). `update` normalizes to plain hashes and reads
  **PascalCase** — `IpProtocol`, `FromPort`, `ToPort`, `IpRanges: [{"CidrIp": …}]`
  (`app/controllers/aws/v1/security_groups_controller.rb:138-157`). Sending the create
  (snake_case) shape to update fails fast with a clean 422 naming the missing `IpProtocol`
  field; sending the update (PascalCase) shape to create yields empty or nil rules.
- **Update validates four sources but only consumes one.** A rule with only `Ipv6Ranges`,
  `PrefixListIds` or `UserIdGroupPairs` passes the controller's source check
  (`security_groups_controller.rb:145-155`) and then hits `perm["IpRanges"].map` on nil
  (`security_group_service.rb:157`). Use `IpRanges`.
- **Update is org-gated by VPC ownership.** Before touching AWS it describes the group and requires a
  `CloudVpc` row for that group's `vpc_id` owned by the org, else 404
  (`security_groups_controller.rb:164-172`). Security groups in the AWS default VPC or created by
  hand in the console are not updateable here. VPC delete is gated the same way
  (`vpcs_controller.rb:66-70`).
- **Delete takes no region.** The VPC delete reads `region` off the stored `CloudVpc`
  (`app/controllers/aws/v1/vpcs_controller.rb:73`); a `region` parameter is ignored.
- **VPC delete cascades and needs an empty VPC.** It detaches and deletes internet gateways, deletes
  every subnet, non-main route tables, non-default network ACLs and non-default security groups, then
  the VPC (`app/services/cloud_provider/aws/vpc_deleter_service.rb:21-61`). Any instance still in it
  makes AWS refuse the delete.
- **Region is not validated.** `VALID_AWS_REGIONS` in the VPC controller is declared and never used
  (`app/controllers/aws/v1/vpcs_controller.rb:99-104`); a bad region surfaces as an AWS error
  (422/500), not a usage error. Subnet and security-group `index`/`create` 422 on a blank region
  (`subnets_controller.rb:14-16`, `security_groups_controller.rb:18-20`); VPC `index`/`create` do not
  — a nil region falls back to the SDK default (`app/services/cloud_provider/aws/concerns/aws_client_builder.rb:95-96`).
- **Subnet create's error message is wrong.** The blank check requires `vpc_id` but the 422 names
  `name` instead, and `name` is only a `Name` tag (`subnets_controller.rb:39-40`,
  `app/services/cloud_provider/aws/subnet_service.rb:31-45`).
- **Subnet CIDRs are derived for a `/16` only.** `"#{cidr.split('.').first(2).join('.')}.#{i}.0/24"`
  ignores your prefix length (`vpc_creator_service.rb:107`); AWS rejects overlaps, and there is no
  app-side conflict check.
- **Lists are region-wide.** `list_subnets` and `list_security_groups` call `describe_subnets` /
  `describe_security_groups` unfiltered (`subnet_service.rb:13-15`, `security_group_service.rb:14-15`),
  so you get the default VPC, the `default` group and groups from every VPC in the region.
- **Every command here is owner-only by default.** The `vpcs`, `subnets` and `security_groups`
  permission keys exist on no seeded role except owner (`{"all" => true}`, `db/seeds.rb:11`); the
  permission backfill migration adds only `projects`/`webhook_endpoints`
  (`db/migrate/20260818090000_add_platform_permissions_to_roles.rb:11-16`). An admin/manager/member
  session gets 403 `You do not have permission to perform this action.`
  (`app/controllers/concerns/authorization_concern.rb:50-51`). The exception is a platform
  admin acting in an explicitly named organization, who passes without a membership
  (`app/controllers/concerns/authorization_concern.rb:35-40`).
- **`cloud_credential_id` is optional.** Omitted (or blank) means platform-managed credentials;
  a credential that belongs to another org raises a 403 (`aws_credential_resolver.rb:18-32`).
- **Credentials logged at debug.** `AwsVpcService` logs the credential hash it was handed
  (`app/services/cloud_provider/aws/aws_vpc_service.rb:13`) — do not leave debug logging on while
  calling the VPC endpoints with BYOC credentials.

## Sources

- Routes: `config/routes.rb:326-390` (`namespace :aws`/`:v1`, `resources :organizations`) — VPC
  `:388`, subnets `:389`, security groups `:390`, org-level `PUT .../security_groups` `:387`;
  instances create `:328`, instances update `:336`.
- Controllers: `app/controllers/aws/v1/vpcs_controller.rb`,
  `subnets_controller.rb`, `security_groups_controller.rb`, `aws/v1/base_controller.rb`,
  `app/controllers/concerns/aws_credential_resolver.rb`, `app/controllers/concerns/response_handler.rb`.
- Services: `app/services/cloud_provider/aws/vpc_creator_service.rb`, `vpc_deleter_service.rb`,
  `aws_vpc_service.rb`, `subnet_service.rb`, `security_group_service.rb`,
  `instance_updaters/network_updater_service.rb`, `concerns/aws_client_builder.rb`.
- Models: `app/models/cloud_vpc.rb`; schema columns `db/schema.rb:669-681`.
- CLI stubs and future verbs: `src/cli/network.rs` (all verbs `not implemented yet`).
- Not verified here: the AWS SDK field names inside the VPC `index` payload (the controller returns
  `vpc.to_h` from the EC2 struct plus `type`, `vpcs_controller.rb:21-23`) and the exact IAM
  permissions a BYOC credential needs.
