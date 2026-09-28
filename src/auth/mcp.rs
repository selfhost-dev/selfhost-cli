//! One-time import of the MCP server's credential file (design §5).
//!
//! `~/.selfhost/credentials.json` is written by `selfhost-mcp` as
//! `{"firebaseApiKey": …, "firebaseRefreshToken": …, "savedAt": …}` at mode
//! `0600`. The CLI only ever reads it: adoption copies the pair into a profile
//! and leaves the file alone, so the MCP server keeps working.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::config::{MCP_CREDENTIALS_FILE, STORE_DIR};
use crate::error::{Error, Result};

use super::Credentials;

/// Credential files are tiny; anything larger is refused before parsing.
const MAX_CREDENTIAL_BYTES: u64 = 64 * 1024;

/// The MCP server's credential file, when a home directory is known.
pub(crate) fn default_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(STORE_DIR).join(MCP_CREDENTIALS_FILE))
}

/// The file's on-disk shape, field names included.
#[derive(Deserialize)]
struct RawCredentials {
    #[serde(rename = "firebaseApiKey")]
    firebase_api_key: String,
    #[serde(rename = "firebaseRefreshToken")]
    firebase_refresh_token: String,
}

/// Read the credentials the MCP server stored at `path`.
pub(crate) fn load_from(path: &Path) -> Result<Credentials> {
    // `symlink_metadata` does not follow symlinks, and anything that is not a
    // regular file (a symlink, FIFO, or device) is refused: `read_to_string`
    // could otherwise be redirected or block forever.
    let meta = fs::symlink_metadata(path).map_err(|err| {
        Error::Other(anyhow::Error::from(err).context(format!("cannot read {}", path.display())))
    })?;
    if !meta.file_type().is_file() {
        return Err(Error::Other(anyhow::anyhow!(
            "{} is not a regular file",
            path.display()
        )));
    }
    if meta.len() > MAX_CREDENTIAL_BYTES {
        return Err(Error::Other(anyhow::anyhow!(
            "{} is larger than a credential file can be",
            path.display()
        )));
    }
    let text = fs::read_to_string(path).map_err(|err| {
        Error::Other(anyhow::Error::from(err).context(format!("cannot read {}", path.display())))
    })?;
    let parsed: RawCredentials = serde_json::from_str(&text).map_err(|_| {
        Error::Other(anyhow::anyhow!(
            "{} is not an MCP credential file",
            path.display()
        ))
    })?;
    if parsed.firebase_api_key.is_empty() || parsed.firebase_refresh_token.is_empty() {
        return Err(Error::Other(anyhow::anyhow!(
            "{} has no firebaseApiKey/firebaseRefreshToken pair",
            path.display()
        )));
    }
    Ok(Credentials {
        api_key: parsed.firebase_api_key,
        refresh_token: parsed.firebase_refresh_token,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_DIR: AtomicU64 = AtomicU64::new(0);

    fn temp_file(tag: &str, contents: &str) -> PathBuf {
        let unique = NEXT_DIR.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "selfhost-mcp-{tag}-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(MCP_CREDENTIALS_FILE);
        fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn the_mcp_file_is_read_as_the_console_wrote_it() {
        let path = temp_file(
            "valid",
            r#"{"firebaseApiKey": "AIza-key", "firebaseRefreshToken": "AMf-token", "savedAt": "2026-09-28T09:00:00.000Z"}"#,
        );

        let credentials = load_from(&path).unwrap();

        assert_eq!(credentials.api_key, "AIza-key");
        assert_eq!(credentials.refresh_token, "AMf-token");
    }

    #[test]
    fn a_missing_malformed_or_incomplete_file_is_refused() {
        let missing = temp_file("missing", "{}").with_extension("nope");
        assert!(load_from(&missing).is_err());

        assert!(load_from(&temp_file("malformed", "{not json")).is_err());
        assert!(load_from(&temp_file("nested", r#"{"firebaseApiKey": {"a": 1}}"#)).is_err());
        assert!(load_from(&temp_file("half", r#"{"firebaseApiKey": "AIza-key"}"#)).is_err());
        assert!(
            load_from(&temp_file(
                "empty",
                r#"{"firebaseApiKey": "", "firebaseRefreshToken": "AMf-token"}"#
            ))
            .is_err()
        );
    }

    #[test]
    fn an_oversized_file_is_refused_before_parsing() {
        let path = temp_file("oversized", &" ".repeat(MAX_CREDENTIAL_BYTES as usize + 1));

        assert!(load_from(&path).is_err());
    }

    #[test]
    fn a_directory_is_refused_as_a_credential_file() {
        let path = temp_file("directory", "{}");
        let dir = path.parent().unwrap();

        assert!(load_from(dir).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_to_the_credential_file_is_refused() {
        let target = temp_file(
            "symlink",
            r#"{"firebaseApiKey": "AIza-key", "firebaseRefreshToken": "AMf-token"}"#,
        );
        let link = target.with_file_name("credentials-link.json");
        std::os::unix::fs::symlink(&target, &link).unwrap();

        let err = load_from(&link).err().expect("a symlink must be refused");
        assert!(err.to_string().contains("not a regular file"), "{err}");
    }
}
