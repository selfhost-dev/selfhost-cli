//! `domain` — org-level custom domains and DNS verification (design §4).

use super::*;

stub_group!(
    /// Org-level custom domains and DNS verification.
    DomainCommand, "domain",
    leaves {
        List(OptionalTargetArgs) => "list",
        Add(DomainRefArgs) => "add",
        Remove(DomainRefArgs) => "remove",
        Verify(DomainRefArgs) => "verify",
        Sync(OptionalTargetArgs) => "sync",
    }
    groups {}
);
