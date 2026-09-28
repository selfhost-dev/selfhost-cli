//! `cloud` — cloud credentials and the default provider (design §4).

use clap::Args;

use super::*;

// `cloud credential add`.
#[derive(Debug, Clone, Args)]
pub struct CredentialAddArgs {
    /// Cloud to add credentials for
    #[arg(long, value_enum)]
    pub provider: Option<Provider>,

    /// Provider access key id
    #[arg(long = "access-key")]
    pub access_key: Option<String>,

    /// Provider secret access key
    #[arg(long = "secret-key")]
    pub secret_key: Option<String>,
}

// `cloud credential update`.
#[derive(Debug, Clone, Args)]
pub struct CredentialUpdateArgs {
    /// Credential id
    pub id: Option<String>,

    /// Provider access key id
    #[arg(long = "access-key")]
    pub access_key: Option<String>,

    /// Provider secret access key
    #[arg(long = "secret-key")]
    pub secret_key: Option<String>,
}

// `cloud credential delete|default`.
#[derive(Debug, Clone, Args)]
pub struct CredentialRefArgs {
    /// Credential id (defaults to the selected credential)
    pub id: Option<String>,
}

stub_group!(
    /// `cloud credential` — provider credentials.
    CloudCredentialCommand, "cloud credential",
    leaves {
        List(NoArgs) => "list",
        Add(CredentialAddArgs) => "add",
        Update(CredentialUpdateArgs) => "update",
        Delete(CredentialRefArgs) => "delete",
        Default(CredentialRefArgs) => "default",
    }
    groups {}
);

stub_group!(
    /// `cloud account` — provider account info.
    CloudAccountCommand, "cloud account",
    leaves {
        Show(NoArgs) => "show",
    }
    groups {}
);

stub_group!(
    /// Cloud credentials and the default provider.
    CloudCommand, "cloud",
    leaves {
    }
    groups {
        Credential(CloudCredentialCommand) => "credential",
        Account(CloudAccountCommand) => "account",
    }
);
