//! Typed HTTP client: one module per resource, the `{status, data, message,
//! status_code}` envelope, `organization_id` injection (query for GET, body for
//! writes), `Retry-After` + backoff handling and the per-operation cooldowns
//! (design §6).
//!
//! Placeholder: Slice 1 (core) fills this in. It is intentionally empty — no HTTP
//! is issued anywhere in the CLI yet.
