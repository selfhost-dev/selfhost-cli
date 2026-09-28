//! `auth` — sign in/out, inspect the session (design §5).

use clap::Args;

use super::*;

// `auth login [--no-browser]` — browser OAuth through `${console_url}/mcp-auth`.
#[derive(Debug, Clone, Args)]
pub struct LoginArgs {
    /// Print the URL and wait instead of opening a browser (SSH: tunnel the port)
    #[arg(long = "no-browser")]
    pub no_browser: bool,
}

stub_group!(
    /// Sign in/out, inspect the session (browser OAuth, per profile).
    AuthCommand, "auth",
    leaves {
        Login(LoginArgs) => "login",
        Logout(NoArgs) => "logout",
        Status(NoArgs) => "status",
        Token(NoArgs) => "token",
    }
    groups {}
);
