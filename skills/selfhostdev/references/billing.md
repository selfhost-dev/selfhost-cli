# Billing

Organization wallet and money path: balance and runway, real-money top-ups, the credit
ledger, usage transactions, the SKU price list, the billing contact, the saved payment
method, auto-recharge and promo-coupon redemption. All of it is org-scoped: the path
carries the organization pid (`org_...`). Money actions need a member with a
billing-level role or higher: the owner, admin and billing roles pass, while manager and
member roles are refused with 422 `You are not authorized to perform this action.` The
exceptions are reading the organization itself (`GET /organizations/{org}` below) and
redeeming a promo coupon — both open to any active member, because the latter only ever
adds credit. The typed commands (`selfhost billing ...`, `selfhost billing auto-recharge
...`) answer `not implemented yet` today, so drive the endpoints below with
`selfhost api` under the rules in [api.md](api.md).

## Before you start

- Signed-in profile: `selfhost auth status`. An organization must be resolved for the
  `{org}` substitution, and on some endpoints (see the trap below) for the request to
  reach the org at all: `selfhost org use <slug>` or `--org <slug|pid>`.
- The path parameter is the org **pid**, not the slug: `set_organization` does
  `Organization.active.find_by(pid: params[:id])` (`app/controllers/organizations_controller.rb:856`).
- `{org}` is the only placeholder and expands to `org_...`; `selfhost api` also appends the
  resolved `organization_id`, which the same controller honours as an org gate
  (`app/controllers/application_controller.rb:123`). Keep both pointed at one org.
- `-X GET` whenever a GET carries parameters, or a bare `-f` turns it into a POST; bodies
  are one flat JSON object, so write `-f`/`-F` keys un-nested.
- Money is integer cents, currency is always `USD` (`organizations_controller.rb:434`,
  `app/services/billing/topup_service.rb:119`). A negative balance is possible: the wallet
  goes into debt rather than missing a debit (`app/services/billing/wallet.rb:25`).
- Send booleans with `-F` (`-F save_card=true`), never `-f`: the controller branches on
  `params[...].present?`, so the string `"false"` counts as true
  (`organizations_controller.rb:506`).

## Endpoint map

Wallet, money and ledger — routes 267-272:

| Future CLI command | Method and path | What it does | Key parameters | Source |
|---|---|---|---|---|
| `selfhost billing balance` | `GET /organizations/{org}/billing/balance` | debt-aware wallet balance, hourly burn, runway, expiry warning, saved card | none | `config/routes.rb:267`, `organizations_controller.rb:418` |
| `selfhost billing topup` | `POST /organizations/{org}/billing/topup` | billable: creates a real-money top-up and the provider checkout payload | `amount_cents`, `billing_contact_phone`, `save_card`, `use_saved_card`, `Idempotency-Key` header | `config/routes.rb:268`, `organizations_controller.rb:444` |
| (second step of `topup`) | `POST /organizations/{org}/billing/topup/complete` | verifies the provider signature and credits the wallet | `order_id`, `razorpay_payment_id`, `razorpay_signature` | `config/routes.rb:269`, `organizations_controller.rb:569` |
| `selfhost billing transactions` | `GET /organizations/{org}/billing/transactions` | usage charges per resource, newest first | `limit` (1-100, default 50) | `config/routes.rb:270`, `organizations_controller.rb:617` |
| `selfhost billing ledger` | `GET /organizations/{org}/billing/ledger` | credit-lot ledger (topups, grants, charges, debt), newest first | `limit` (1-100, default 50) | `config/routes.rb:271`, `organizations_controller.rb:643` |
| `selfhost billing skus` | `GET /organizations/{org}/billing/skus` | active price list, optionally with this org's margin | `db_type`, `region` | `config/routes.rb:272`, `organizations_controller.rb:667` |

Billing contact and payment method — routes 256, 264-266:

| Future CLI command | Method and path | What it does | Key parameters | Source |
|---|---|---|---|---|
| `selfhost billing contact` | `GET /organizations/{org}` | read the org (billing fields are plain columns on it; any active member can read, no billing role needed) | none | `config/routes.rb:256`, `organizations_controller.rb:141` |
| (billing contact update) | `PATCH /organizations/{org}/billing/contact` | updates the billing contact fields only | `billing_contact_phone`, `tax_id`, `country_code`, `state_code` | `config/routes.rb:266`, `organizations_controller.rb:408` |
| (second step of a payment-method link) | `POST /organizations/{org}/billing/link/complete` | completes an already-started provider link and makes the channel chargeable | `order_id`, `payment_id`, `razorpay_signature` | `config/routes.rb:265`, `organizations_controller.rb:370` |
| `selfhost billing unlink-payment` | `DELETE /organizations/{org}/billing/link` | destructive: deletes the stored provider token and resets the link state | none | `config/routes.rb:264`, `organizations_controller.rb:339` |

Auto-recharge — routes 273-277:

| Future CLI command | Method and path | What it does | Key parameters | Source |
|---|---|---|---|---|
| `selfhost billing auto-recharge show` | `GET /organizations/{org}/billing/auto_recharge` | settings, saved cards, burn, per-charge cap | none | `config/routes.rb:273`, `organizations_controller.rb:712` |
| `selfhost billing auto-recharge set` | `POST /organizations/{org}/billing/auto_recharge` | create-or-update (upsert) the settings row | `threshold_cents`, `amount_cents`, `monthly_cap_cents`, `token_id`, `provider` | `config/routes.rb:274`, `organizations_controller.rb:720` |
| `selfhost billing auto-recharge set` | `PATCH /organizations/{org}/billing/auto_recharge` | same upsert; use for an existing row | same | `config/routes.rb:275`, `organizations_controller.rb:730` |
| `selfhost billing auto-recharge enable` | `POST /organizations/{org}/billing/auto_recharge/enable` | turns charging on; refuses without a linked token | none (needs `-X POST`) | `config/routes.rb:276`, `organizations_controller.rb:740` |
| `selfhost billing auto-recharge disable` | `POST /organizations/{org}/billing/auto_recharge/disable` | turns charging off; a no-op when no row exists | none (needs `-X POST`) | `config/routes.rb:277`, `organizations_controller.rb:750` |

Promo coupon — route 258:

| Future CLI command | Method and path | What it does | Key parameters | Source |
|---|---|---|---|---|
| `selfhost billing redeem-coupon` | `POST /organizations/{org}/coupon_redemptions` | redeems a promo code into this org's wallet (any active member, not billing-role gated) | `code` | `config/routes.rb:258`, `app/controllers/coupon_redemptions_controller.rb:18` |

## Top up end to end

```sh
# 1. read-only: balance, burn and the saved card before spending anything
selfhost api /organizations/{org}/billing/balance --org acme -o json
# -> balance_cents, balance_formatted, currency, payment_channel_state,
#    hourly_burn_rate_cents, estimated_hours_remaining, expiring_soon, card

# 2. BILLABLE: create the top-up ($25.00). In this path nothing is charged yet;
#    the response carries the provider checkout payload.
selfhost api /organizations/{org}/billing/topup --org acme \
  -F amount_cents=2500 \
  -f billing_contact_phone="$BILLING_PHONE" \
  -H "Idempotency-Key: $TOPUP_KEY" -o json
# -> topup_pid, amount_cents, status, save_card, payment_url (always null here),
#    checkout { provider_type, key, amount, currency, order_id, customer_id, prefill,
#    topup_pid, notes, recurring }

# 3. the customer pays in the browser checkout built from data.checkout, then the
#    three values that checkout hands back complete the top-up. This is the
#    irreversible, real-money step.
selfhost api /organizations/{org}/billing/topup/complete --org acme \
  -f order_id="$RAZORPAY_ORDER_ID" \
  -f razorpay_payment_id="$RAZORPAY_PAYMENT_ID" \
  -f razorpay_signature="$RAZORPAY_SIGNATURE" -o json
# -> topup_pid, amount_cents, status, balance_cents, payment_channel_state

# 4. confirm: the balance rose and a `topup` ledger entry exists
selfhost api -X GET /organizations/{org}/billing/ledger --org acme -F limit=5
```

One-tap variant: pass `-F use_saved_card=true` in step 2 and skip step 3 — the linked card
is charged off-session. A captured charge returns `status: "completed"` inline; a bank
challenge returns HTTP 202 with `status: "pending_confirmation"`, `payment_id` and
`order_id`, and the `payment.captured` webhook finishes it later (up to 1 working day,
`app/services/billing/topup_charge_classifier.rb:2-19`). If the charge is pending, poll
`billing/balance` or `billing/ledger`; there is no top-up status endpoint.

## Redeem a promo coupon

```sh
# redeem: any active member may do this, and the wallet is credited in the same call
selfhost api /organizations/{org}/coupon_redemptions --org acme \
  -f code="$PROMO_CODE" -o json
# -> code, amount_cents, new_balance_cents

# the credit lands as an `adjustment` lot; confirm it in the ledger
selfhost api -X GET /organizations/{org}/billing/ledger --org acme -F limit=5
```

Codes are case- and whitespace-insensitive (trimmed and upcased before the lookup).
There is no endpoint that lists the codes or checks one without spending it; the
catalog is administered platform-side.

## Day-2 operations

```sh
# update the billing contact (phone is also the one-tap/save-card requirement)
selfhost api -X PATCH /organizations/{org}/billing/contact --org acme \
  -f billing_contact_phone="$BILLING_PHONE" -f country_code=US -f state_code=CA

# page the ledger and the usage transactions (only `limit` is honoured)
selfhost api -X GET /organizations/{org}/billing/ledger --org acme -F limit=20
selfhost api -X GET /organizations/{org}/billing/transactions --org acme -F limit=20

# price list; db_type also resolves this org's margin override
selfhost api -X GET /organizations/{org}/billing/skus --org acme -f db_type=postgres

# configure auto-recharge, then enable it (token_id comes from `cards[].token_id`)
selfhost api /organizations/{org}/billing/auto_recharge --org acme \
  -F threshold_cents=1000 -F amount_cents=5000 -F monthly_cap_cents=20000 \
  -f token_id="$TOKEN_ID"
selfhost api -X POST /organizations/{org}/billing/auto_recharge/enable --org acme
selfhost api -X POST /organizations/{org}/billing/auto_recharge/disable --org acme

# destructive: drop the saved payment method (kills one-tap and auto-recharge)
selfhost api -X DELETE /organizations/{org}/billing/link --org acme
```

## Gotchas

- **Billable and irreversible**: `billing/topup` starts a real charge; `topup/complete`
  credits the wallet, nets outstanding debt first, and cannot be undone
  (`app/services/billing/credit_service.rb:143`). There is no cancel endpoint.
- **Top-up bounds**: `amount_cents` must be 500-50 000 ($5-$500), integer cents
  (`topup_service.rb:3`). Outside the range: 422 `Minimum top-up amount is $5.00` /
  `Maximum top-up amount is $500.00`.
- **Rate limit**: `billing_topup` allows 5 requests per minute per user+org
  (`organizations_controller.rb:30`) — 429, and the CLI replays once, then exits 75.
- **Idempotency**: send `Idempotency-Key` or `X-Idempotency-Key` on `billing/topup`. A
  repeat with the same key and same payload replays the stored response for 24 h; a
  concurrent repeat gets 422 `A top-up with this Idempotency-Key is already in progress.
  Re-submit after it completes.`; a *different* payload under the same
  key is treated as a new top-up (the fingerprint includes `amount_cents`, `save_card`,
  `use_saved_card`), so use a fresh key per intended charge (`organizations_controller.rb:453`,
  `:466`, `:491`).
- **`save_card` needs a phone**: `save_card=true` without a usable `billing_contact_phone`
  fails 422 with `code`/`field` before any charge (`topup_service.rb:81`). `use_saved_card`
  without a linked card is 422 `saved_card_unavailable`; above the card mandate ceiling it
  is 422 `mandate_limit_exceeded` with `data.mandate_max_cents`
  (`topup_service.rb:12`, `topup_service.rb:39`).
- **Provider outage is 402**, which the CLI exits 4 (`billing required: ...`).
- **`payment_url` is always null** in this codebase; nothing writes that metadata key
  (`organizations_controller.rb:536`). Use `checkout` for the browser flow.
- **Auto-recharge validation**: `amount_cents` 500-15 000 ($5-$150 per charge),
  `threshold_cents` >= 100 and `monthly_cap_cents` >= `amount_cents` >= 100;
  `token_id` must be a card owned by this org; `enable` refuses when no `token_id` is
  stored or a legacy amount exceeds the $150 cap
  (`app/services/billing/auto_recharge_settings_service.rb:9`, `:68`, `:134`). `POST` and
  `PATCH` run the same upsert.
- **Debt netting**: a $25 top-up on a wallet that owes $10 raises the balance by $15, and
  the ledger row can carry `amount_cents: 0` while the top-up itself is `completed`
  (`credit_service.rb:144`).
- **`page` is not read** by `billing/transactions` or `billing/ledger`; only `limit`
  (default 50, clamped 1-100). No date filter is read either, so the future verbs'
  `--page`/`--since` do nothing here (`organizations_controller.rb:624`).
- **`db_type` is validated**: an unknown value is 422 `Invalid db_type`
  (`organizations_controller.rb:673`); accepted values come from
  `DatabaseAdapters::Registry.supported_types` (`app/services/database_adapters/registry.rb:3`).
- **`billing/link/complete` needs a pending setup** in the org's settings that no customer
  endpoint in this repo creates; without it the call is 422 `Payment method link setup not
  found` (`app/services/billing/payment_method_link_completer.rb:139`). Note it takes
  `payment_id`, not `razorpay_payment_id` (`organizations_controller.rb:809`).
- **Coupon redemption is a member action, not a billing one**: it adds credit only, so
  owner, admin, billing, manager and member roles all pass; only a non-member is turned
  away, and that is a 404 on the org, not a 422 (`coupon_redemptions_controller.rb:3-7`).
  An unverified email is refused up front with 422 and `data.code`
  `email_verification_required` — its message talks about creating an organization, which
  is just reused boilerplate (`app/controllers/concerns/email_verification_gate.rb:80`).
- **Coupon codes cannot be pre-checked**: a wrong, expired, deactivated or capped-out
  code is one generic 422 `This coupon code is invalid or no longer available`, so
  unknown and used-up codes are deliberately indistinguishable
  (`coupon_redemptions_controller.rb:42`). One organization can redeem each code once ever:
  the repeat is 422 `Your organization has already redeemed this coupon`
  (`coupon_redemptions_service.rb:43`). A missing `code` is 422 `code is required`.
- **Coupon credit behaves like a top-up**: outstanding wallet debt is netted first, so
  the ledger lot can carry `amount_cents: 0` while the response's `new_balance_cents`
  still rises (`app/services/billing/credit_service.rb:124`). The lot is an
  `adjustment` row described `Promo coupon: <CODE>` and expires after the coupon's
  `credit_expires_in_days` when one is set (`app/services/billing/credit_service.rb:128`).
- **Coupon rate limit**: 10 redemption attempts per hour per user — 429, and the CLI
  replays once, then exits 75 (`coupon_redemptions_controller.rb:14`). Guessing burns
  the budget, so redeem known codes one at a time.
- **Redemption can be switched off server-side**: when the deployment turns coupons off
  every code answers the generic invalid message, with no client-visible flag
  (`app/services/coupons.rb:6`).
- **Org gate differs per action**: `billing_topup_complete` and every `auto_recharge`
  action are not in the `skip_before_action` list (`organizations_controller.rb:4`), so
  they need `@current_organization` — satisfied by the injected `organization_id`, or by
  the pid in the path on these `/organizations/:id/...` routes
  (`application_controller.rb:129`). A pid the caller is not an active member of resolves
  to nil and answers 404 `Organization not found or not specified.`
  (`application_controller.rb:140`). A `{org}` path with no organization resolved never
  reaches the server: the CLI exits 2 with `no organization selected; run selfhost org use
  <slug>` ([api.md](api.md)).
- **Credentials**: the checkout payload's `key` is the public provider key from
  `ENV["RAZORPAY_KEY_ID"]` (`app/services/billing/configuration.rb:11`); the secret is never
  returned. There is no test/live switch in the code — whichever key the deployment
  configured is live for real money. Do not paste responses containing `key` or card data
  into logs or files.
- A second, platform-admin-only unlink exists at `DELETE /admin/organizations/:organization_pid/billing/link`
  (`config/routes.rb:212`, `app/controllers/admin/organization_billing_links_controller.rb:3`)
  and calls the same unlinker; it is not reachable with a customer profile.
- The coupon catalog behind the codes is platform-admin only: list, create and update at
  `/admin/coupons` plus a per-coupon redemption list at `GET /admin/coupons/:pid/redemptions`
  (`config/routes.rb:216-218`, `app/controllers/admin/coupons_controller.rb:4`). There is
  no show or delete endpoint and none of it is reachable with a customer profile, so there
  is no way to list or validate a code from the CLI.

## Sources

- `config/routes.rb:256-277` (top-level `resources :organizations` + the billing member block), `:258` (coupon redemption), `:212`, `:216-218` (admin-only unlink and coupon catalog)
- `app/controllers/organizations_controller.rb:4-32` (skip list, org resolution, top-up rate limit), `:339`, `:370`, `:408`, `:418`, `:444`, `:569`, `:617`, `:643`, `:667`, `:712-763`, `:790-816` (permitted params)
- `app/controllers/application_controller.rb:112-173` (organization resolution by `organization_id`/`organization_pid`/`id`)
- `app/controllers/concerns/response_handler.rb:7-73` (`{status, data, message, status_code}` envelope, 422)
- `app/services/billing/topup_service.rb:3`, `:63-88`, `:357-358` (bounds, saved-card gates, validation)
- `app/services/billing/topup_charge_classifier.rb:2-30` (captured / pending_confirmation / declined)
- `app/services/billing/wallet.rb:25`, `app/services/billing/credit_service.rb:23`, `:143` (balance, expiring_soon, top-up credit)
- `app/services/billing/auto_recharge_settings_service.rb:9-16`, `:68`, `:134`, `:170-215` (validation, present shape)
- `app/services/billing/payment_method_link_completer.rb:139`, `app/services/billing/payment_method_unlinker.rb:16`
- `app/controllers/coupon_redemptions_controller.rb:3-16`, `:18-43` (membership-scoped comment, auth + verified-email gate, per-user rate limit, `code`-only body, 422 mapping)
- `app/services/billing/coupon_redemption_service.rb:14-48` (cap, one-per-org unique index, credit grant, `AlreadyRedeemed`)
- `app/services/coupons.rb:3-15` (`enabled?` kill switch, admin amount cap), `app/models/coupon.rb:14-51` (code format, redeemable rules)
- `app/controllers/concerns/email_verification_gate.rb:15`, `:79-83` (`email_verification_required` 422)
- `app/services/billing/providers/razorpay_provider.rb:182-205` (checkout payload), `app/services/billing/configuration.rb:8-23` (keys)
- Models: `app/models/topup.rb:6`, `app/models/credit_ledger.rb:6-7`, `app/models/billing_transaction.rb:9`, `app/models/sku.rb:45`, `app/models/billing/auto_recharge_setting.rb:35`, `app/models/organization.rb:6-14`, `app/models/user.rb:61-84` (priority-based role gate), `db/seeds.rb` (owner 100, admin 80, billing 60, manager 40)
- `app/services/database_adapters/registry.rb:3-12`, `:40` (accepted `db_type` values)
