//! Authentication: loopback login through `${console_url}/mcp-auth?port=N`, the
//! `/callback` handoff, `securetoken.googleapis.com` refresh-token exchange, the
//! in-memory ID-token cache with its 5-minute buffer, rotated-refresh-token
//! persistence, and the one-time import of the MCP `~/.selfhost/credentials.json`
//! (design §5).
//!
//! Placeholder: Slice 1 (core) fills this in. `selfhost auth login` currently
//! answers `not implemented yet: auth login`.
