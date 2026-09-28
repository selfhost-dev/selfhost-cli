//! `domain` — org-level custom domains and DNS verification (design §4).

use super::*;

stub_group!(
    /// Custom domains for your organization and their DNS status
    DomainCommand, "domain",
    leaves {
        /// List domains
        List(OptionalTargetArgs) => "list",
        /// Add a domain
        Add(DomainRefArgs) => "add",
        /// Remove a domain
        Remove(DomainRefArgs) => "remove",
        /// Check a domain's DNS
        Verify(DomainRefArgs) => "verify",
        /// Re-sync domains with the platform
        Sync(OptionalTargetArgs) => "sync",
    }
    groups {}
);
