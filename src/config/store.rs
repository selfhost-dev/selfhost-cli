//! Profile store I/O for `~/.selfhost/config.json` (design §5).
//!
//! Slice 1 owns what the Slice 0 model deliberately left out: seeding the
//! built-ins as ordinary profiles, `0700`/`0600` permissions that do not
//! depend on the process umask, atomic writes that cannot be redirected
//! through a symlink, endpoint validation (https everywhere, plain http only
//! for loopback), and profile resolution (`--profile` — which clap already
//! merges with `SELFHOSTDEV_PROFILE` — then `default_profile`).

use std::fs::{self, DirBuilder, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

use super::{Config, Profile, builtin_profile};

/// Directory under `$HOME` that holds this store (and MCP's credential file).
pub const STORE_DIR: &str = ".selfhost";
/// Store file name inside [`STORE_DIR`].
pub const STORE_FILE: &str = "config.json";
/// Profiles seeded on first run. `default` mirrors production so that a
/// first-ever command always has somewhere to point.
pub const SEEDED_PROFILES: [&str; 4] = ["default", "prod", "qa", "local"];
/// Credential files are tiny; anything larger is refused before parsing.
const MAX_CONFIG_BYTES: u64 = 1 << 20;

/// The on-disk profile store.
///
/// `Debug` is safe here: [`Profile`]'s own `Debug` redacts the credentials
/// (`config::tests::debug_output_hides_credentials_through_config` pins that).
#[derive(Debug)]
pub struct ProfileStore {
    path: PathBuf,
    config: Config,
}

impl ProfileStore {
    /// `~/.selfhost/config.json`.
    pub fn default_path() -> Result<PathBuf> {
        let home = dirs::home_dir()
            .ok_or_else(|| Error::Other(anyhow::anyhow!("cannot determine your home directory")))?;
        Ok(home.join(STORE_DIR).join(STORE_FILE))
    }

    /// Load the default store; a missing file yields the seeded profile set.
    pub fn load() -> Result<Self> {
        Self::at(Self::default_path()?)
    }

    /// Load (or seed) the store at an explicit path.
    pub fn at(path: PathBuf) -> Result<Self> {
        let config = if path.exists() {
            read_config(&path)?
        } else {
            seeded()
        };
        Ok(Self { path, config })
    }

    /// Where the store lives.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The whole configuration.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// One profile by name.
    pub fn profile(&self, name: &str) -> Option<&Profile> {
        self.config.profiles.get(name)
    }

    /// One profile by name, mutably, for credential writes.
    pub fn profile_mut(&mut self, name: &str) -> Option<&mut Profile> {
        self.config.profiles.get_mut(name)
    }

    /// One profile by name, or the usage error that names the fix.
    pub fn require_profile(&self, name: &str) -> Result<&Profile> {
        self.profile(name).ok_or_else(|| unknown_profile(name))
    }

    /// Insert or replace a profile.
    pub fn insert_profile(&mut self, name: &str, profile: Profile) {
        self.config.profiles.insert(name.to_string(), profile);
    }

    /// Remove a profile; `true` when it existed.
    pub fn remove_profile(&mut self, name: &str) -> bool {
        self.config.profiles.remove(name).is_some()
    }

    /// The profile used when none is selected.
    pub fn default_profile_name(&self) -> &str {
        &self.config.default_profile
    }

    /// Point the default at `name`.
    pub fn set_default_profile(&mut self, name: &str) {
        self.config.default_profile = name.to_string();
    }

    /// Resolve the profile name for this run: explicit selection (which clap
    /// already resolved from `--profile` over `SELFHOSTDEV_PROFILE`) first,
    /// then `default_profile`. Unknown names are refused — profiles are never
    /// created implicitly.
    pub fn resolved_name(&self, selected: Option<&str>) -> Result<String> {
        // An empty selector (`--profile ''`/`SELFHOSTDEV_PROFILE=''`) means "not
        // set", not a profile literally named "".
        let name = match selected.filter(|name| !name.is_empty()) {
            Some(name) => name.to_string(),
            None => self.config.default_profile.clone(),
        };
        self.require_profile(&name)?;
        Ok(name)
    }

    /// [`Self::resolved_name`] plus the profile itself.
    pub fn resolved(&self, selected: Option<&str>) -> Result<(String, &Profile)> {
        let name = self.resolved_name(selected)?;
        let profile = self
            .profile(&name)
            .expect("resolved_name checked that the profile exists");
        Ok((name, profile))
    }

    /// Write the store: `~/.selfhost` at `0700`, `config.json` at `0600`,
    /// replaced atomically through a fresh `O_EXCL` temp file in the same
    /// directory so a planted symlink cannot redirect the write.
    pub fn save(&self) -> Result<()> {
        let dir = self
            .path
            .parent()
            .ok_or_else(|| Error::Other(anyhow::anyhow!("{} has no parent directory", self.path.display())))?;
        create_dir_0700(dir)?;

        let json = serde_json::to_string_pretty(&self.config)
            .map_err(|err| Error::Other(anyhow::Error::from(err).context("cannot serialize the profile store")))?;
        let tmp = dir.join(format!(".{STORE_FILE}.tmp.{}", std::process::id()));

        // A previous run that died between creating and renaming may have left a
        // stale temp file behind; clear it so `create_new` can succeed.
        let _ = fs::remove_file(&tmp);

        let written = (|| -> std::io::Result<()> {
            let mut file = open_new_0600(&tmp)?;
            file.write_all(json.as_bytes())?;
            file.write_all(b"\n")?;
            file.sync_all()
        })();
        if let Err(err) = written {
            let _ = fs::remove_file(&tmp);
            return Err(Error::Other(
                anyhow::Error::from(err).context(format!("cannot write {}", tmp.display())),
            ));
        }
        if let Err(err) = fs::rename(&tmp, &self.path) {
            let _ = fs::remove_file(&tmp);
            return Err(Error::Other(
                anyhow::Error::from(err)
                    .context(format!("cannot replace {}", self.path.display())),
            ));
        }
        // The rename preserves the temp file's `0600`; this repeats it in case
        // the platform ignored the create mode.
        set_mode(&self.path, 0o600)
    }
}

/// The store a first run starts with: the four seeded profiles.
fn seeded() -> Config {
    let mut config = Config::empty();
    for name in SEEDED_PROFILES {
        let source = if name == "default" { "prod" } else { name };
        let profile = builtin_profile(source)
            .expect("SEEDED_PROFILES only names built-ins and prod");
        config.profiles.insert(name.to_string(), profile);
    }
    config
}

fn unknown_profile(name: &str) -> Error {
    Error::Usage(format!(
        "unknown profile '{name}'; create it first: selfhost profile add {name}"
    ))
}

/// Read and parse one store file, refusing symlinks, oversized files and
/// invalid UTF-8, and repairing loose permissions before trusting contents.
fn read_config(path: &Path) -> Result<Config> {
    // A symlinked store directory can redirect the read to an attacker-chosen
    // file; refuse it, mirroring the write path's guard in `create_dir_0700`.
    if let Some(dir) = path.parent()
        && let Ok(dir_meta) = fs::symlink_metadata(dir)
        && dir_meta.file_type().is_symlink()
    {
        return Err(Error::Other(anyhow::anyhow!(
            "refusing to read {}: {} is a symbolic link",
            path.display(),
            dir.display()
        )));
    }
    let meta = fs::symlink_metadata(path)
        .map_err(|err| Error::Other(anyhow::Error::from(err).context(format!("cannot inspect {}", path.display()))))?;
    if meta.file_type().is_symlink() {
        return Err(Error::Other(anyhow::anyhow!(
            "refusing to read {}: it is a symbolic link",
            path.display()
        )));
    }
    if meta.len() > MAX_CONFIG_BYTES {
        return Err(Error::Other(anyhow::anyhow!(
            "{} is larger than {MAX_CONFIG_BYTES} bytes; refusing to parse it",
            path.display()
        )));
    }
    repair_loose_permissions(path)?;

    let bytes = fs::read(path)
        .map_err(|err| Error::Other(anyhow::Error::from(err).context(format!("cannot read {}", path.display()))))?;
    let text = String::from_utf8(bytes).map_err(|_| {
        Error::Other(anyhow::anyhow!("{} is not valid UTF-8; refusing to parse it", path.display()))
    })?;
    serde_json::from_str(&text).map_err(|err| {
        Error::Other(
            anyhow::Error::from(err).context(format!("cannot parse {}", path.display())),
        )
    })
}

/// Bring group/other-readable store permissions back to `0600`/`0700` before
/// the credentials inside are trusted, and say so on stderr.
fn repair_loose_permissions(path: &Path) -> Result<()> {
    if let Some(dir) = path.parent()
        && let Ok(meta) = fs::symlink_metadata(dir)
        && meta.is_dir()
        && loose(dir)?
    {
        set_mode(dir, 0o700)?;
        eprintln!("note: tightened permissions on {}", dir.display());
    }
    if loose(path)? {
        set_mode(path, 0o600)?;
        eprintln!("note: tightened permissions on {}", path.display());
    }
    Ok(())
}

#[cfg(unix)]
fn loose(path: &Path) -> Result<bool> {
    use std::os::unix::fs::PermissionsExt as _;
    let mode = fs::metadata(path)
        .map_err(|err| Error::Other(anyhow::Error::from(err).context(format!("cannot inspect {}", path.display()))))?
        .permissions()
        .mode();
    Ok(mode & 0o077 != 0)
}

#[cfg(not(unix))]
fn loose(_path: &Path) -> Result<bool> {
    Ok(false)
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).map_err(|err| {
        Error::Other(anyhow::Error::from(err).context(format!("cannot set permissions on {}", path.display())))
    })
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _mode: u32) -> Result<()> {
    Ok(())
}

fn create_dir_0700(dir: &Path) -> Result<()> {
    match fs::symlink_metadata(dir) {
        Ok(meta) => {
            if meta.file_type().is_symlink() {
                return Err(Error::Other(anyhow::anyhow!(
                    "refusing to write through {}: it is a symbolic link",
                    dir.display()
                )));
            }
            if !meta.is_dir() {
                return Err(Error::Other(anyhow::anyhow!(
                    "{} exists and is not a directory",
                    dir.display()
                )));
            }
            // An already-existing directory is tightened before anything is
            // written into it, not only on a later read.
            if loose(dir)? {
                set_mode(dir, 0o700)?;
            }
            Ok(())
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            let mut builder = DirBuilder::new();
            builder.recursive(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt as _;
                builder.mode(0o700);
            }
            builder.create(dir).map_err(|err| {
                Error::Other(anyhow::Error::from(err).context(format!("cannot create {}", dir.display())))
            })
        }
        Err(err) => Err(Error::Other(
            anyhow::Error::from(err).context(format!("cannot inspect {}", dir.display())),
        )),
    }
}

fn open_new_0600(path: &Path) -> std::io::Result<std::fs::File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    options.open(path)
}

/// Validate `base_url`/`console_url` values (design §5b): absolute `https`,
/// except plain `http` for loopback hosts — a downgrade to `http` anywhere
/// else fails closed instead of silently sending credentials in the clear.
pub fn validate_endpoint(label: &str, url: &str) -> Result<()> {
    let Some((scheme, rest)) = url.split_once("://") else {
        return Err(Error::Other(anyhow::anyhow!(
            "{label} must be an absolute https URL (got '{url}')"
        )));
    };
    // Any `@` in the authority is user info; `http://localhost:3000@evil.com`
    // would otherwise satisfy the loopback exemption while pointing at evil.com.
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.contains('@') {
        return Err(Error::Other(anyhow::anyhow!(
            "{label} must not carry user info (got '{url}')"
        )));
    }
    match scheme.to_ascii_lowercase().as_str() {
        "https" if !host(rest).is_empty() => Ok(()),
        "http" if is_loopback(host(rest)) => Ok(()),
        "http" => Err(Error::Other(anyhow::anyhow!(
            "{label} must use https (got '{url}'); plain http is only allowed for localhost"
        ))),
        other => Err(Error::Other(anyhow::anyhow!(
            "{label} must use https (got '{other}://')"
        ))),
    }
}

/// Host part of a URL body (`host[:port][/path]`), IPv6 brackets included.
fn host(rest: &str) -> &str {
    if let Some(stripped) = rest.strip_prefix('[') {
        return match stripped.find(']') {
            Some(end) => &rest[..end + 2],
            None => rest,
        };
    }
    rest.split(['/', ':', '?', '#']).next().unwrap_or("")
}

fn is_loopback(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "::1" | "[::1]")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_DIR: AtomicU64 = AtomicU64::new(0);

    fn temp_dir(tag: &str) -> PathBuf {
        let unique = NEXT_DIR.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|since| since.as_nanos())
            .unwrap_or_default();
        let dir = std::env::temp_dir().join(format!(
            "selfhost-store-{tag}-{}-{unique}-{nanos}",
            std::process::id()
        ));
        DirBuilder::new().recursive(true).create(&dir).unwrap();
        #[cfg(unix)]
        set_mode(&dir, 0o700).unwrap();
        dir
    }

    #[test]
    fn first_run_seeds_default_prod_qa_local() {
        let store = ProfileStore::at(temp_dir("seed").join(STORE_FILE)).unwrap();

        for name in SEEDED_PROFILES {
            assert!(store.profile(name).is_some(), "{name} must be seeded");
        }
        assert_eq!(store.default_profile_name(), "default");
        assert_eq!(
            store.profile("default").unwrap().base_url,
            "https://api.selfhost.dev"
        );
        assert_eq!(store.profile("qa").unwrap().base_url, "https://qapi.selfhost.dev");
        assert_eq!(store.profile("local").unwrap().base_url, "http://localhost:3000");
        assert!(store.profile("local").unwrap().console_url.is_none());
    }

    #[test]
    fn resolution_prefers_the_selected_profile_and_refuses_unknown_names() {
        let store = ProfileStore::at(temp_dir("resolve").join(STORE_FILE)).unwrap();

        assert_eq!(store.resolved_name(None).unwrap(), "default");
        assert_eq!(store.resolved_name(Some("qa")).unwrap(), "qa");

        let err = store.resolved_name(Some("staging")).unwrap_err();
        assert!(matches!(err, Error::Usage(_)));
        assert!(err.to_string().contains("profile add staging"));
    }

    #[cfg(unix)]
    #[test]
    fn save_writes_0600_and_0700() {
        use std::os::unix::fs::PermissionsExt as _;

        let dir = temp_dir("perms");
        let store_path = dir.join(STORE_FILE);
        let store = ProfileStore::at(store_path.clone()).unwrap();
        store.save().unwrap();

        let file_mode = fs::metadata(&store_path).unwrap().permissions().mode();
        assert_eq!(file_mode & 0o777, 0o600, "config.json must be 0600");
        let dir_mode = fs::metadata(&dir).unwrap().permissions().mode();
        assert_eq!(dir_mode & 0o777, 0o700, "~/.selfhost must be 0700");
    }

    #[cfg(unix)]
    #[test]
    fn save_replaces_a_planted_symlink_without_writing_through_it() {
        let dir = temp_dir("symlink");
        let victim = dir.join("victim.txt");
        fs::write(&victim, "untouched").unwrap();

        let store_path = dir.join(STORE_FILE);
        let store = ProfileStore::at(store_path.clone()).unwrap();
        store.save().unwrap();

        // A later run finds a symlink where the store used to be; loading that
        // path is refused by design, so only `save` may repair it.
        fs::remove_file(&store_path).unwrap();
        std::os::unix::fs::symlink(&victim, &store_path).unwrap();
        store.save().unwrap();

        assert_eq!(fs::read_to_string(&victim).unwrap(), "untouched");
        let meta = fs::symlink_metadata(&store_path).unwrap();
        assert!(
            meta.file_type().is_file(),
            "save must leave a regular file where the symlink was"
        );
        let reloaded = ProfileStore::at(store_path).unwrap();
        assert_eq!(reloaded.default_profile_name(), "default");
    }

    #[test]
    fn oversized_store_is_refused() {
        let path = temp_dir("oversize").join(STORE_FILE);
        fs::write(&path, vec![b' '; (MAX_CONFIG_BYTES + 1) as usize]).unwrap();
        let err = ProfileStore::at(path).unwrap_err();
        assert!(err.to_string().contains("larger than"));
    }

    #[test]
    fn invalid_utf8_store_is_refused() {
        let path = temp_dir("utf8").join(STORE_FILE);
        fs::write(&path, [0xff, 0xfe, 0x00]).unwrap();
        let err = ProfileStore::at(path).unwrap_err();
        assert!(err.to_string().contains("not valid UTF-8"));
    }

    #[test]
    fn roundtrip_preserves_credentials_and_settings() {
        let dir = temp_dir("roundtrip");
        let path = dir.join(STORE_FILE);
        let mut store = ProfileStore::at(path.clone()).unwrap();
        let mut profile = store.require_profile("qa").unwrap().clone();
        profile.firebase_api_key = Some("api-key-canary".to_string());
        profile.firebase_refresh_token = Some("refresh-canary".to_string());
        profile.org = Some("acme".to_string());
        store.insert_profile("qa", profile);
        store.save().unwrap();

        let reloaded = ProfileStore::at(path).unwrap();
        let qa = reloaded.require_profile("qa").unwrap();
        assert_eq!(qa.firebase_api_key.as_deref(), Some("api-key-canary"));
        assert_eq!(qa.firebase_refresh_token.as_deref(), Some("refresh-canary"));
        assert_eq!(qa.org.as_deref(), Some("acme"));
    }

    #[test]
    fn profile_mut_edits_the_named_profile_in_place() {
        let mut store = ProfileStore::at(temp_dir("mutate").join(STORE_FILE)).unwrap();

        store.profile_mut("qa").expect("qa is seeded").org = Some("acme".to_string());

        assert_eq!(store.profile("qa").unwrap().org.as_deref(), Some("acme"));
        assert!(store.profile_mut("no-such-profile").is_none());
    }

    #[test]
    fn endpoint_validation_matrix() {
        assert!(validate_endpoint("base_url", "https://api.selfhost.dev").is_ok());
        assert!(validate_endpoint("base_url", "https://api.selfhost.dev/path").is_ok());
        assert!(validate_endpoint("base_url", "http://localhost:3000").is_ok());
        assert!(validate_endpoint("base_url", "http://127.0.0.1:8080").is_ok());
        assert!(validate_endpoint("base_url", "http://[::1]:3000").is_ok());
        // The scheme is matched case-insensitively.
        assert!(validate_endpoint("base_url", "HTTPS://api.selfhost.dev").is_ok());
        assert!(validate_endpoint("base_url", "HTTPS://API.selfhost.dev/path").is_ok());

        assert!(validate_endpoint("base_url", "http://api.selfhost.dev").is_err());
        assert!(validate_endpoint("base_url", "ftp://api.selfhost.dev").is_err());
        assert!(validate_endpoint("base_url", "api.selfhost.dev").is_err());
        assert!(validate_endpoint("base_url", "https://").is_err());
    }

    #[test]
    fn userinfo_in_the_authority_never_passes_validation() {
        // Each of these would otherwise satisfy the loopback `http` exemption
        // while actually pointing at evil.com.
        for bypass in [
            "http://localhost:3000@evil.com",
            "http://127.0.0.1:3000@evil.com",
            "http://[::1]:3000@evil.com",
        ] {
            let err = validate_endpoint("base_url", bypass).unwrap_err();
            assert!(
                err.to_string().contains("user info"),
                "{bypass} must be refused: {err}"
            );
        }
        // `@` is rejected on https too: it is never part of a host.
        assert!(validate_endpoint("base_url", "https://user@api.selfhost.dev").is_err());
    }

    #[test]
    fn an_empty_profile_selection_falls_back_to_the_default() {
        let store = ProfileStore::at(temp_dir("empty-select").join(STORE_FILE)).unwrap();
        assert_eq!(store.resolved_name(Some("")).unwrap(), "default");
        assert_eq!(store.resolved_name(None).unwrap(), "default");
    }

    #[test]
    fn save_replaces_a_stale_temp_file() {
        let dir = temp_dir("stale-tmp");
        let store_path = dir.join(STORE_FILE);
        let store = ProfileStore::at(store_path.clone()).unwrap();
        let tmp = dir.join(format!(".{STORE_FILE}.tmp.{}", std::process::id()));
        fs::write(&tmp, "stale").unwrap();

        store.save().unwrap();

        assert!(!tmp.exists(), "the stale temp file is cleared");
        let reloaded = ProfileStore::at(store_path).unwrap();
        assert_eq!(reloaded.default_profile_name(), "default");
    }

    #[cfg(unix)]
    #[test]
    fn save_refuses_a_symlinked_store_directory() {
        let dir = temp_dir("symlink-dir");
        let real = dir.join("real");
        fs::create_dir_all(&real).unwrap();
        let link = dir.join(STORE_DIR);
        std::os::unix::fs::symlink(&real, &link).unwrap();

        let store = ProfileStore::at(link.join(STORE_FILE)).unwrap();
        let err = store.save().unwrap_err();

        assert!(err.to_string().contains("symbolic link"), "{err}");
        assert!(
            !real.join(STORE_FILE).exists(),
            "nothing may be written through the symlinked directory"
        );
    }

    #[cfg(unix)]
    #[test]
    fn read_refuses_a_symlinked_store_directory() {
        let dir = temp_dir("symlink-dir-read");
        let real = dir.join("real");
        fs::create_dir_all(&real).unwrap();
        let store_path = real.join(STORE_FILE);
        ProfileStore::at(store_path.clone()).unwrap().save().unwrap();

        let link = dir.join(STORE_DIR);
        std::os::unix::fs::symlink(&real, &link).unwrap();

        let err = ProfileStore::at(link.join(STORE_FILE)).unwrap_err();
        assert!(err.to_string().contains("symbolic link"), "{err}");

        // The same file under a real directory still reads.
        let reloaded = ProfileStore::at(store_path).unwrap();
        assert_eq!(reloaded.default_profile_name(), "default");
    }

    #[cfg(unix)]
    #[test]
    fn save_tightens_an_existing_loose_store_directory() {
        use std::os::unix::fs::PermissionsExt as _;

        let dir = temp_dir("loose-dir");
        let store_dir = dir.join(STORE_DIR);
        fs::create_dir_all(&store_dir).unwrap();
        set_mode(&store_dir, 0o755).unwrap();

        let store = ProfileStore::at(store_dir.join(STORE_FILE)).unwrap();
        store.save().unwrap();

        let mode = fs::metadata(&store_dir).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700, "an existing ~/.selfhost must be 0700");
    }
}
