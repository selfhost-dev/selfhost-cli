//! `cloud` — cloud credentials and the default provider (design §4).

use clap::Args;

use super::*;

// `cloud credential add`.
#[derive(Clone, Args)]
pub struct CredentialAddArgs {
    /// Cloud to add credentials for
    #[arg(long, value_enum)]
    pub provider: Option<Provider>,

    /// Access key id
    #[arg(long = "access-key")]
    pub access_key: Option<String>,

    /// Secret access key. Prefer SELFHOSTDEV_CLOUD_SECRET_KEY; flag values stay visible in shell history and the process list.
    #[arg(
        long = "secret-key",
        env = "SELFHOSTDEV_CLOUD_SECRET_KEY",
        hide_env_values = true
    )]
    pub secret_key: Option<String>,
}

impl std::fmt::Debug for CredentialAddArgs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CredentialAddArgs")
            .field("provider", &self.provider)
            .field("access_key", &self.access_key)
            .field(
                "secret_key",
                &self.secret_key.as_ref().map(|_| "[REDACTED]"),
            )
            .finish()
    }
}

// `cloud credential update`.
#[derive(Clone, Args)]
pub struct CredentialUpdateArgs {
    /// Credential id
    pub id: Option<String>,

    /// Access key id
    #[arg(long = "access-key")]
    pub access_key: Option<String>,

    /// Secret access key. Prefer SELFHOSTDEV_CLOUD_SECRET_KEY; flag values stay visible in shell history and the process list.
    #[arg(
        long = "secret-key",
        env = "SELFHOSTDEV_CLOUD_SECRET_KEY",
        hide_env_values = true
    )]
    pub secret_key: Option<String>,
}

impl std::fmt::Debug for CredentialUpdateArgs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CredentialUpdateArgs")
            .field("id", &self.id)
            .field("access_key", &self.access_key)
            .field(
                "secret_key",
                &self.secret_key.as_ref().map(|_| "[REDACTED]"),
            )
            .finish()
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct AddProbe {
        #[command(flatten)]
        args: CredentialAddArgs,
    }

    #[test]
    fn debug_output_hides_cloud_secret_key() {
        let add = CredentialAddArgs {
            provider: None,
            access_key: Some("AKIA-plain".to_string()),
            secret_key: Some("canary-cloud-secret-4d5e6f".to_string()),
        };
        let shown = format!("{add:?}");
        assert!(!shown.contains("canary-cloud-secret-4d5e6f"));
        assert!(shown.contains("AKIA-plain"));

        let update = CredentialUpdateArgs {
            id: Some("cred-1".to_string()),
            access_key: Some("AKIA-plain".to_string()),
            secret_key: Some("canary-cloud-secret-7a8b9c".to_string()),
        };
        let shown = format!("{update:?}");
        assert!(!shown.contains("canary-cloud-secret-7a8b9c"));
        assert!(shown.contains("cred-1"));
    }

    #[test]
    fn secret_key_env_fallback_parses() {
        unsafe { std::env::set_var("SELFHOSTDEV_CLOUD_SECRET_KEY", "env-canary-secret") };
        let probe = AddProbe::try_parse_from(["probe"]).expect("env fallback must parse");
        assert_eq!(probe.args.secret_key.as_deref(), Some("env-canary-secret"));
        unsafe { std::env::remove_var("SELFHOSTDEV_CLOUD_SECRET_KEY") };
    }
}
