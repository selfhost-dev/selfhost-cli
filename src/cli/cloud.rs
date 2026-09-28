//! `cloud` — cloud credentials and the default provider (design §4).

use clap::Args;

use super::*;

// `cloud credential add`.
#[derive(Debug, Clone, Args)]
pub struct CredentialAddArgs {
    /// Cloud to add credentials for
    #[arg(long, value_enum)]
    pub provider: Option<Provider>,

    /// Access key id
    #[arg(long = "access-key")]
    pub access_key: Option<String>,

    /// Secret access key
    #[arg(long = "secret-key")]
    pub secret_key: Option<String>,
}

// `cloud credential update`.
#[derive(Debug, Clone, Args)]
pub struct CredentialUpdateArgs {
    /// Credential id
    pub id: Option<String>,

    /// Access key id
    #[arg(long = "access-key")]
    pub access_key: Option<String>,

    /// Secret access key
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
    /// Credentials SelfHost uses in your cloud account
    CloudCredentialCommand, "cloud credential",
    leaves {
        /// List credentials
        List(NoArgs) => "list",
        /// Add credentials
        Add(CredentialAddArgs) => "add",
        /// Change credentials
        Update(CredentialUpdateArgs) => "update",
        /// Delete credentials
        Delete(CredentialRefArgs) => "delete",
        /// Make a credential the default
        Default(CredentialRefArgs) => "default",
    }
    groups {}
);

stub_group!(
    /// Details of your cloud account
    CloudAccountCommand, "cloud account",
    leaves {
        /// Show your cloud account
        Show(NoArgs) => "show",
    }
    groups {}
);

stub_group!(
    /// Cloud provider credentials and the default provider
    CloudCommand, "cloud",
    leaves {
    }
    groups {
        Credential(CloudCredentialCommand) => "credential",
        Account(CloudAccountCommand) => "account",
    }
);
