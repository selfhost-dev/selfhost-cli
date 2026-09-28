//! `auth` — sign in/out, inspect the session (design §5).

use clap::Args;

use super::*;

// `auth login [--no-browser]` — browser OAuth through `${console_url}/mcp-auth`.
#[derive(Debug, Clone, Args)]
pub struct LoginArgs {
    /// Print the sign-in URL instead of opening a browser
    #[arg(long = "no-browser")]
    pub no_browser: bool,
}

stub_group!(
    /// Sign in, sign out and inspect the current session
    AuthCommand, "auth",
    leaves {
        /// Sign in through your browser
        Login(LoginArgs) => "login",
        /// Sign out of the current profile
        Logout(NoArgs) => "logout",
        /// Show who you are signed in as
        Status(NoArgs) => "status",
        /// Print the current access token
        Token(NoArgs) => "token",
    }
    groups {}
);
