//! Poll loops behind `--wait` and `--follow`, with the documented cadences
//! (projects 10s, provisioning 4s, scale 5–10s with a 30-minute cap, backups 20s
//! with a 30-minute cap) and the server-side cooldowns they must never beat
//! (design §6). ActionCable streaming is reserved for v2.
//!
//! Placeholder: Slice 6 fills this in.
