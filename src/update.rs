//! `update` — replace the running build with the newest published stable one.
//!
//! The release index (`https://cli.selfhost.dev/latest.json` by default, the same
//! file the installers read) names one download per platform and the SHA-256 of
//! each. This module turns that into an outcome: compare, fetch, verify, and
//! swap the executable at `target` in place.
//!
//! Three rules shape the whole flow, and each one is load-bearing:
//!
//! * the index decides *which* release, never *where from* — a download that
//!   does not come from the index's own host is refused, exactly like the
//!   origin pin in `distribution/install.sh`, so a tampered index cannot send
//!   the update somewhere else;
//! * nothing is verified, nothing is written, until the bytes hash to the
//!   published value — a mismatch leaves the installed build byte-for-byte as
//!   it was;
//! * the CLI never elevates. A path it cannot write is answered with the
//!   installer command that will work, not with `sudo` and not with a shell.
//!
//! Two things here look like duplication and are not: the `https`-unless-
//! loopback rule also lives in `config::store::validate_endpoint` (which
//! decides where API requests go, this one where release bytes come from), so
//! a change to one policy belongs in the other; and the client below is built
//! here rather than reused, because this one has its own policy — redirects
//! off, and budgets of its own — and must never follow a redirect whatever
//! the API clients do.
//!
//! What the digest does *not* prove, stated here because it cannot be fixed in
//! this file. The index, the download URL and the digest all arrive over one
//! TLS session from one origin, so anyone who owns `cli.selfhost.dev`, the
//! identity it presents, or the trust anchor the client accepts defeats the
//! digest check and every other control on this page at once: a digest that
//! matches an attacker-supplied download proves only that the download is the
//! one the index named. Compromise of that origin is code execution on every
//! client that updates, and no amount of checking inside the client changes
//! that. The real fix is a digest that does not come from the same origin —
//! signed by a key the index host does not hold, or attested by a separate
//! signer — so that compromising one party does not speak for the other.

use std::fs;
use std::io::{Read as _, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::anyhow;
use semver::Version;
use serde::Deserialize;
use sha2::{Digest as _, Sha256};

use crate::error::{Error, Result};

/// Where the release index lives when nothing overrides it.
pub const DEFAULT_MANIFEST_URL: &str = "https://cli.selfhost.dev/latest.json";

/// The environment variable the installers honour, and the fallback this
/// command reads when `--manifest-url` is absent.
pub const MANIFEST_URL_ENV: &str = "SELFHOST_MANIFEST_URL";

/// The command that installs a build into a directory this one cannot write.
const INSTALLER_POSIX: &str = "curl -fsSL https://cli.selfhost.dev/install.sh | sh";
const INSTALLER_WINDOWS: &str = "irm https://cli.selfhost.dev/install.ps1 | iex";

/// How long a connection may take to establish.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// The release index is a few kilobytes, so it is read under its own budget.
const MANIFEST_TIMEOUT: Duration = Duration::from_secs(20);
/// A release binary is a few megabytes, so it gets the larger budget.
const ARTIFACT_TIMEOUT: Duration = Duration::from_secs(300);

/// The most this CLI will read from a release index. The published list is a
/// few kilobytes, so a megabyte is generous; the cap is there so a host that
/// answers with a video cannot make the CLI buffer it.
const MAX_INDEX_BYTES: usize = 1024 * 1024;

/// The most this CLI will read from a release download. A release binary is a
/// few megabytes; a quarter of a gigabyte leaves room for a debug build or a
/// future platform bundle and still refuses an endless body.
const MAX_ARTIFACT_BYTES: usize = 256 * 1024 * 1024;

/// What each request is called in a refusal, so a message names the step that
/// failed rather than a URL.
const INDEX: &str = "the release index";
const DOWNLOAD: &str = "the release download";

/// The archive as a refusal names it: what is being unpacked, not what was
/// downloaded, because that is the step that is running out of room.
const ARCHIVE: &str = "the release archive";

/// The name the release index uses for this build, or `None` when the CLI was
/// compiled for a platform it publishes nothing for.
pub fn platform_key() -> Option<&'static str> {
    platform_key_for(std::env::consts::OS, std::env::consts::ARCH)
}

/// The platform key for one `os`/`arch` pair, as `std::env::consts` spells them.
fn platform_key_for(os: &str, arch: &str) -> Option<&'static str> {
    match (os, arch) {
        ("linux", "x86_64") => Some("linux-x86_64"),
        ("linux", "aarch64") => Some("linux-aarch64"),
        ("macos", "x86_64") => Some("macos-x86_64"),
        ("macos", "aarch64") => Some("macos-aarch64"),
        ("windows", "x86_64") => Some("windows-x86_64"),
        ("windows", "aarch64") => Some("windows-aarch64"),
        _ => None,
    }
}

/// One run's inputs, with the target path kept out: the same update logic
/// serves the real binary and a test's temporary file.
#[derive(Debug, Clone)]
pub struct Config {
    /// The release index to read.
    pub manifest_url: String,
    /// Report the comparison and download nothing.
    pub check_only: bool,
    /// The version of the build asking to be updated — `CARGO_PKG_VERSION` in
    /// the binary, an arbitrary version in a test.
    pub local: Version,
    /// The global `--timeout`, in seconds, folded into a duration.
    pub request_timeout: Duration,
}

impl Config {
    /// The configuration of a real `selfhost update` run.
    pub fn new(check_only: bool, request_timeout: Duration) -> Self {
        Self {
            manifest_url: DEFAULT_MANIFEST_URL.to_string(),
            check_only,
            local: local_version(),
            request_timeout,
        }
    }
}

/// The version of this build. `Cargo.toml` is semver by construction, so a
/// parse failure is a packaging bug rather than a runtime condition.
fn local_version() -> Version {
    Version::parse(env!("CARGO_PKG_VERSION"))
        .expect("the crate version is valid semver, enforced by cargo publish")
}

/// The release index URL: the flag, then the environment variable, then the
/// published default. An empty value counts as unset, like `--base-url`.
pub fn resolve_manifest_url(explicit: Option<&str>, from_env: Option<&str>) -> String {
    for candidate in [explicit, from_env] {
        if let Some(url) = candidate.map(str::trim).filter(|url| !url.is_empty()) {
            return url.to_string();
        }
    }
    DEFAULT_MANIFEST_URL.to_string()
}

/// What an update run did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The running build is the newest published release.
    UpToDate {
        /// The release the index names.
        latest: Version,
    },
    /// The running build is newer than anything published.
    LocalIsNewer {
        /// The release the index names.
        latest: Version,
    },
    /// The running build is a pre-release, and the index only carries stable ones.
    PreRelease {
        /// The release the index names.
        latest: Version,
    },
    /// `--check`: both versions and whether a newer stable release exists.
    Checked {
        /// The running build.
        current: Version,
        /// The release the index names.
        latest: Version,
        /// Whether an update is available.
        available: bool,
    },
    /// The executable at the target was replaced.
    Installed {
        /// The release now in place.
        version: Version,
        /// The path that was replaced.
        target: PathBuf,
        /// On Windows the previous build, which a running image cannot delete.
        backup: Option<PathBuf>,
    },
}

/// The release index, as published.
#[derive(Debug, Deserialize)]
struct Manifest {
    version: String,
    #[serde(default)]
    assets: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    sha256: std::collections::BTreeMap<String, String>,
}

/// A parsed, validated location a download may come from.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Source {
    url: reqwest::Url,
    host: String,
}

/// A run of `selfhost update` against one executable.
///
/// The target is a parameter, not something read from the process: the CLI
/// hands in `std::env::current_exe()`, and a test hands in a temporary file.
pub async fn run(config: &Config, target: &Path) -> Result<Outcome> {
    let index = Source::parse(&config.manifest_url, "the release index address")?;

    let manifest = fetch_manifest(config, &index).await?;
    let latest = Version::parse(manifest.version.trim()).map_err(|err| {
        Error::Other(anyhow!(
            "the release index names a version this CLI cannot read ({}): {err}",
            manifest.version
        ))
    })?;

    // `--check` reports the comparison whatever it is, so an up-to-date install
    // still answers with both versions and downloads nothing.
    if config.check_only {
        return Ok(Outcome::Checked {
            current: config.local.clone(),
            latest: latest.clone(),
            available: update_available(&config.local, &latest),
        });
    }

    if is_pre_release(&config.local) {
        return Ok(Outcome::PreRelease { latest });
    }
    match config.local.cmp(&latest) {
        std::cmp::Ordering::Equal => return Ok(Outcome::UpToDate { latest }),
        std::cmp::Ordering::Greater => return Ok(Outcome::LocalIsNewer { latest }),
        std::cmp::Ordering::Less => {}
    }

    let key = platform_key().ok_or_else(|| {
        Error::Other(anyhow!(
            "no release is published for this platform ({}-{})",
            std::env::consts::OS,
            std::env::consts::ARCH
        ))
    })?;
    let asset = manifest
        .assets
        .get(key)
        .ok_or_else(|| missing_platform(key, "download"))?;
    let expected = normalize_digest(
        manifest
            .sha256
            .get(key)
            .ok_or_else(|| missing_platform(key, "published SHA-256"))?,
    )?;
    // The pin the installers enforce: the index may choose the release, but it
    // may not move the download off the origin it was read from — scheme, host
    // and port alike, so another port on the same host is another site.
    let download = Source::parse(asset, "the download address")?;
    if download.origin() != index.origin() {
        return Err(Error::Other(anyhow!(
            "the release index points at {}, which is not the same site it was read from",
            without_user_info(asset),
        )));
    }

    install(config, &download, &expected, latest, target).await
}

/// The refusal for a platform the release index says nothing about.
fn missing_platform(key: &str, what: &str) -> Error {
    Error::Other(anyhow!(
        "the release index has no {what} for {key} on this platform"
    ))
}

/// Whether a stable newer release is waiting: a pre-release never updates, and
/// anything at or past the published version is up to date.
fn update_available(local: &Version, latest: &Version) -> bool {
    !is_pre_release(local) && local < latest
}

/// Whether a version carries a pre-release tag. Only stable releases are
/// published, so a pre-release never moves to another one.
fn is_pre_release(version: &Version) -> bool {
    !version.pre.is_empty()
}

/// The published SHA-256, lowercased. Anything that is not 64 hex characters
/// is a malformed index rather than a build to install.
fn normalize_digest(raw: &str) -> Result<String> {
    let trimmed = raw.trim();
    if trimmed.len() != 64 || !trimmed.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Error::Other(anyhow!(
            "the release index does not carry a valid SHA-256 for this platform"
        )));
    }
    Ok(trimmed.to_ascii_lowercase())
}

impl Source {
    /// Parse a download location: absolute, `https` (or `http` on loopback, so a
    /// test can serve an index locally), and carrying no user info —
    /// `http://localhost:1@evil.example` would otherwise satisfy the loopback
    /// exemption while pointing somewhere else entirely.
    fn parse(raw: &str, label: &str) -> Result<Self> {
        let url = reqwest::Url::parse(raw.trim()).map_err(|err| {
            Error::Other(anyhow!(
                "{label} is not a valid URL ({}): {err}",
                without_user_info(raw)
            ))
        })?;
        if !url.username().is_empty() || url.password().is_some() {
            return Err(Error::Other(anyhow!(
                "{label} must not carry a username or password ({})",
                without_user_info(raw),
            )));
        }
        let host = url
            .host_str()
            .ok_or_else(|| {
                Error::Other(anyhow!("{label} has no host ({})", without_user_info(raw)))
            })?
            .to_ascii_lowercase();
        let allowed = url.scheme() == "https" || (url.scheme() == "http" && is_loopback(&host));
        if !allowed {
            return Err(Error::Other(anyhow!(
                "{label} must use https (got {}); plain http is only allowed on localhost",
                without_user_info(raw),
            )));
        }
        Ok(Self { url, host })
    }

    /// The origin a download is pinned to: scheme, host and effective port.
    /// The port is the one in force, so `https://host/x` and
    /// `https://host:443/y` are the same site while `:8443` is not.
    fn origin(&self) -> (String, String, Option<u16>) {
        (
            self.url.scheme().to_string(),
            self.host.clone(),
            self.url.port_or_known_default(),
        )
    }
}

/// Whether a host is this machine, the one exemption from `https`.
fn is_loopback(host: &str) -> bool {
    // `Url::host_str` keeps the brackets around an IPv6 host, so both
    // spellings of `::1` belong here.
    matches!(host, "localhost" | "127.0.0.1" | "::1" | "[::1]")
}

/// A URL with any user info dropped: `https://user:secret@host/…` reads as
/// `https://host/…`. Every refusal names the address this way — the parser
/// exists to say *that* a password is not allowed, not to print it.
fn without_user_info(raw: &str) -> String {
    let trimmed = raw.trim();
    let Some((scheme, rest)) = trimmed.split_once("://") else {
        return trimmed.to_string();
    };
    let (authority, tail) = match rest.find(['/', '?', '#']) {
        Some(at) => rest.split_at(at),
        None => (rest, ""),
    };
    match authority.rsplit_once('@') {
        Some((_, host)) => format!("{scheme}://{host}{tail}"),
        None => trimmed.to_string(),
    }
}

/// The HTTP client every request here uses: one client per call so each
/// request carries its own budget, and redirects off, so a 3xx is answered
/// rather than followed to wherever it points.
fn client(timeout: Duration) -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(timeout)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        // Building is not a recoverable condition here: a default client would
        // silently drop the redirect pin and the timeouts with it.
        .map_err(|err| {
            Error::Other(anyhow::Error::from(err).context("cannot build the HTTP client"))
        })
}

/// The manifest read budget: the global `--timeout` is a ceiling, so a smaller
/// one tightens it and a larger one does not loosen the documented budget.
fn manifest_timeout(config: &Config) -> Duration {
    MANIFEST_TIMEOUT.min(config.request_timeout)
}

/// The download budget: the global `--timeout` is a floor, because a release
/// binary is not an API response and must not fail on a run sized for one.
fn artifact_timeout(config: &Config) -> Duration {
    ARTIFACT_TIMEOUT.max(config.request_timeout)
}

/// A transport failure, without the URL echoed back by the client.
fn transport_error(what: &str, err: reqwest::Error) -> Error {
    Error::Other(anyhow::Error::from(err.without_url()).context(format!("cannot reach {what}")))
}

/// The status gate both requests pass through, named for the request that hit
/// it. The client has redirects off, so a 3xx reaching this point is the
/// server's answer and is refused rather than chased; anything else that is
/// not a success is refused with its status.
fn gate_status(what: &str, response: &reqwest::Response) -> Result<()> {
    let status = response.status();
    if status.is_redirection() {
        return Err(Error::Other(anyhow!(
            "{what} answered HTTP {status} instead of the file it should have; a redirect is not followed"
        )));
    }
    if !status.is_success() {
        return Err(Error::Other(anyhow!("{what} answered HTTP {status}")));
    }
    Ok(())
}

/// Read a response body into `out`, refusing anything past `cap` bytes.
///
/// The cap is enforced twice on purpose: a declared `Content-Length` over it
/// is refused before a single body byte is read, and the running total is
/// checked on every chunk so a server that declares nothing cannot stream one
/// forever. On refusal the caller has written nothing worth keeping — the
/// download lives in a [`TempDir`] that is removed on the way out, and the
/// target has not been touched.
async fn read_capped<W: Write>(
    mut response: reqwest::Response,
    what: &str,
    cap: usize,
    out: &mut W,
) -> Result<()> {
    if let Some(declared) = response.content_length()
        && declared > cap as u64
    {
        return Err(too_large(what, cap, declared));
    }
    let mut read: u64 = 0;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|err| transport_error(what, err))?
    {
        read += chunk.len() as u64;
        if read > cap as u64 {
            return Err(too_large(what, cap, read));
        }
        out.write_all(&chunk)
            .map_err(|err| fs_fail(err, "cannot write what was downloaded"))?;
    }
    Ok(())
}

/// The refusal for a body past the cap, with the sizes named so the user can
/// tell a hostile answer from a too-small limit.
fn too_large(what: &str, cap: usize, got: u64) -> Error {
    Error::Other(anyhow!(
        "{what} is {got} bytes, past the {} this CLI will read",
        human_size(cap)
    ))
}

/// A byte count as a person reads it.
fn human_size(bytes: usize) -> String {
    if bytes >= 1024 * 1024 && bytes.is_multiple_of(1024 * 1024) {
        format!("{} MiB", bytes / (1024 * 1024))
    } else {
        format!("{bytes} bytes")
    }
}

/// Read and parse the release index.
async fn fetch_manifest(config: &Config, index: &Source) -> Result<Manifest> {
    let response = client(manifest_timeout(config))?
        .get(index.url.clone())
        .send()
        .await
        .map_err(|err| transport_error(INDEX, err))?;
    gate_status(INDEX, &response)?;
    // Bounded bytes, not `text()`: a body that is not UTF-8 is a bad document,
    // not a network failure, and `from_slice` says so where `text` would have
    // reported it as an unreadable response.
    let mut body = Vec::new();
    read_capped(response, INDEX, MAX_INDEX_BYTES, &mut body).await?;
    serde_json::from_slice(&body).map_err(|err| {
        Error::Other(anyhow!(
            "{INDEX} is not the release list this CLI reads: {err}"
        ))
    })
}

/// Fetch, verify and install one release, or leave the target untouched.
async fn install(
    config: &Config,
    download: &Source,
    expected: &str,
    version: Version,
    target: &Path,
) -> Result<Outcome> {
    // The download lands in a private directory that is removed on the way out
    // of this function, whatever the outcome — so a cap hit, a hash mismatch
    // or a refused replacement all clean up after themselves.
    let directory = TempDir::new("selfhost-update")?;
    let downloaded = directory.path().join("download");
    let response = client(artifact_timeout(config))?
        .get(download.url.clone())
        .send()
        .await
        .map_err(|err| transport_error(DOWNLOAD, err))?;
    gate_status(DOWNLOAD, &response)?;
    let mut file = create_private(&downloaded)?;
    read_capped(response, DOWNLOAD, MAX_ARTIFACT_BYTES, &mut file).await?;
    file.sync_all()
        .map_err(|err| fs_fail(err, "cannot write the download"))?;
    drop(file);

    // Verified before anything is written next to the target: a mismatch here
    // means the installed build has not been touched at all.
    let actual = sha256_file(&downloaded)?;
    if actual != expected {
        return Err(Error::Other(anyhow!(
            "the downloaded release does not match its published SHA-256; nothing was installed"
        )));
    }

    // Windows publishes a zip whose published hash covers the archive, exactly
    // like the PowerShell installer verifies it: the executable comes out of
    // the verified archive, never straight from the response body.
    let payload = if cfg!(windows) {
        let exe = directory.path().join("selfhost.exe");
        extract_executable(&downloaded, &exe, MAX_ARTIFACT_BYTES)?;
        exe
    } else {
        downloaded
    };

    let mut staged = stage_next_to(target)?;
    let backup = match replace_from(&mut staged, &payload, expected, target) {
        Ok(backup) => backup,
        Err(err) => {
            // `replace_from` closes the handle on every path out, so the
            // staging name can be removed here and never outlives the run.
            let _ = fs::remove_file(&staged.path);
            return Err(err);
        }
    };
    Ok(Outcome::Installed {
        version,
        target: target.to_path_buf(),
        backup,
    })
}

/// A file nobody else can read, for bytes that are about to become a build.
fn create_private(path: &Path) -> Result<fs::File> {
    fs::File::create(path).map_err(|err| fs_fail(err, "cannot write the download"))
}

/// Write the verified build into the staging file and put it in place.
///
/// The handle comes from [`stage_next_to`], which opened the file with
/// `create_new`, and is taken rather than reopened: a path another process
/// could swap for a symlink between the two steps would redirect a write made
/// by name, and the handle cannot be redirected. It is also closed before the
/// rename, because a file something still holds open cannot be replaced — and
/// closing it reopens exactly the window [`verify_staged`] closes at the end
/// of this function.
fn replace_from(
    staged: &mut Staged,
    payload: &Path,
    expected: &str,
    target: &Path,
) -> Result<Option<PathBuf>> {
    let mut file = staged.file.take().ok_or_else(|| {
        fs_fail(
            std::io::Error::other("the staging file is already closed"),
            "cannot write the new build",
        )
    })?;
    copy_file(payload, &mut file)?;
    file.sync_all()
        .map_err(|err| fs_fail(err, "cannot write the new build"))?;
    drop(file);
    set_executable(&staged.path)?;
    verify_staged(staged, expected)?;
    swap(staged.path.clone(), target)
}

/// The last check before the rename: the bytes sitting at the staging path are
/// still the ones that were verified a moment ago.
///
/// The staging name is predictable — the pid, plus a sweep of sixteen — so
/// anything with write access to the binary's own directory could put a
/// different file there between the write and the rename. An open descriptor
/// cannot close that window, because a file with an open handle cannot be
/// renamed over on Windows. Re-hashing the path immediately before the rename
/// is what closes it: whatever is at that name at that instant is either the
/// verified build or it is refused. A refusal removes the file and installs
/// nothing.
fn verify_staged(staged: &Staged, expected: &str) -> Result<()> {
    let actual = sha256_file(&staged.path)?;
    if actual == expected {
        return Ok(());
    }
    let _ = fs::remove_file(&staged.path);
    Err(Error::Other(anyhow!(
        "the file beside the target changed while the update was running; nothing was installed"
    )))
}

/// SHA-256 of a file, streamed so a release binary is never held in memory.
fn sha256_file(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path).map_err(|err| fs_fail(err, "cannot read the download"))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|err| fs_fail(err, "cannot read the download"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Take the executable out of a release archive.
///
/// The archive must hold exactly one entry, named `selfhost.exe`, at its root.
/// Any name carrying a separator or `..` is refused before it can name anything
/// outside the temporary directory, and everything written stays inside it.
///
/// `cap` bounds what lands on disk, not what arrived over the wire: a few
/// hundred kilobytes of archive can inflate to anything, so a declared size
/// over the cap is refused before a byte is written and the copy stops if the
/// bytes still overrun.
fn extract_executable(archive: &Path, out: &Path, cap: usize) -> Result<()> {
    let file =
        fs::File::open(archive).map_err(|err| fs_fail(err, "cannot read the release archive"))?;
    let mut zip = zip::ZipArchive::new(std::io::BufReader::new(file)).map_err(|err| {
        Error::Other(anyhow!(
            "the Windows release is not a readable archive: {err}"
        ))
    })?;
    if zip.len() != 1 {
        return Err(Error::Other(anyhow!(
            "the Windows release archive holds {} entries; it must hold exactly one",
            zip.len()
        )));
    }
    let mut entry = zip
        .by_index(0)
        .map_err(|err| Error::Other(anyhow!("cannot read the Windows release archive: {err}")))?;
    let name = entry.name().to_string();
    // Two separate refusals, because two separate things are wrong: a name
    // with a separator names something *inside* the archive that is not at its
    // root, and a name that is a `..` component tries to climb out of the
    // directory the entry would be written to.
    if name.contains('/') || name.contains('\\') {
        return Err(Error::Other(anyhow!(
            "the Windows release archive contains an entry outside its root ({name})"
        )));
    }
    if name.split('/').any(|part| part == "..") {
        return Err(Error::Other(anyhow!(
            "the Windows release archive contains an entry that climbs out of it ({name})"
        )));
    }
    if name != "selfhost.exe" {
        return Err(Error::Other(anyhow!(
            "the Windows release archive does not contain selfhost.exe"
        )));
    }
    if entry.size() > cap as u64 {
        return Err(too_large(ARCHIVE, cap, entry.size()));
    }
    let mut out =
        fs::File::create(out).map_err(|err| fs_fail(err, "cannot unpack the release archive"))?;
    let mut capped = Capped::new(&mut out, cap);
    match std::io::copy(&mut entry, &mut capped) {
        Ok(_) => {}
        Err(err) if err.kind() == std::io::ErrorKind::FileTooLarge => {
            return Err(too_large(ARCHIVE, cap, capped.written));
        }
        Err(err) => {
            return Err(fs_fail(err, "cannot unpack the release archive"));
        }
    }
    out.flush()
        .map_err(|err| fs_fail(err, "cannot unpack the release archive"))?;
    Ok(())
}

/// A writer that refuses to pass on more than `cap` bytes.
///
/// The wire cap does not bound what a compressed archive expands to, so the
/// count is kept while the bytes move rather than trusted from the entry's
/// declared size.
struct Capped<W> {
    inner: W,
    cap: usize,
    written: u64,
}

impl<W: Write> Capped<W> {
    fn new(inner: W, cap: usize) -> Self {
        Self {
            inner,
            cap,
            written: 0,
        }
    }
}

impl<W: Write> Write for Capped<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.written += buf.len() as u64;
        if self.written > self.cap as u64 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::FileTooLarge,
                "past the cap",
            ));
        }
        self.inner.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

/// A staging file: the name it was created under, and the handle it was
/// created with. The handle is an `Option` because it has to be closed before
/// the file can be renamed or removed — a file something still holds open is a
/// file that cannot be replaced.
struct Staged {
    path: PathBuf,
    file: Option<fs::File>,
}

/// Write a verified build beside the target, never onto it.
///
/// `create_new` is the point: an existing path is never followed or truncated,
/// so a name that is already there — or a symlink planted there — cannot
/// redirect the write somewhere else. The handle travels with the name so
/// nothing ever reopens it by path. The name is `<target>.new-<pid>`, with a
/// bounded suffix sweep for a leftover from an interrupted run.
fn stage_next_to(target: &Path) -> Result<Staged> {
    let name = target
        .file_name()
        .ok_or_else(|| {
            Error::Other(anyhow!(
                "{} is not a file this CLI can replace",
                target.display()
            ))
        })?
        .to_string_lossy()
        .into_owned();
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    let pid = std::process::id();

    let mut last: Option<std::io::Error> = None;
    for attempt in 0..16 {
        let suffix = match attempt {
            0 => String::new(),
            other => format!("-{other}"),
        };
        let staged = parent.join(format!("{name}.new-{pid}{suffix}"));
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staged)
        {
            Ok(file) => {
                return Ok(Staged {
                    path: staged,
                    file: Some(file),
                });
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                last = Some(err);
                continue;
            }
            Err(err) if is_permission_error(&err) => {
                return Err(cannot_write(target, err));
            }
            Err(err) => return Err(io_error(target, err)),
        }
    }
    Err(Error::Other(
        anyhow::Error::new(last.unwrap_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::AlreadyExists, "no free staging name")
        }))
        .context(format!(
            "cannot stage the new build next to {}",
            target.display()
        )),
    ))
}

/// Make the staged build runnable.
#[cfg(unix)]
fn set_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).map_err(|err| {
        Error::Other(
            anyhow::Error::new(err).context(format!("cannot make {} executable", path.display())),
        )
    })
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) -> Result<()> {
    Ok(())
}

/// Replace the target with the staged build, atomically where the platform
/// allows it. Returns the previous build on Windows, which a running image
/// cannot delete.
#[cfg(unix)]
fn swap(staged: PathBuf, target: &Path) -> Result<Option<PathBuf>> {
    match fs::rename(&staged, target) {
        Ok(()) => Ok(None),
        Err(err) => {
            let _ = fs::remove_file(&staged);
            if is_permission_error(&err) {
                return Err(cannot_write(target, err));
            }
            Err(io_error(target, err))
        }
    }
}

/// A running Windows image can be renamed but not deleted, so the old build
/// moves aside first and stays there for the caller to remove.
#[cfg(windows)]
fn swap(staged: PathBuf, target: &Path) -> Result<Option<PathBuf>> {
    let previous = free_sidecar(target, "old")?;
    swap_keeping_previous(&staged, target, &previous)
}

/// Move the target aside, move the staged build in, and put the original back
/// if that fails.
///
/// The sequence is platform-independent on purpose: it is the part with no
/// rollback of its own, so it is written once, here, and tested on every
/// platform rather than only on the one that needs it. The two renames are not
/// atomic together — there is a moment with no build at the target — so the
/// failure path restores the original and, if even that fails, says exactly
/// where the original is rather than leaving the user to search.
#[cfg(any(windows, test))]
fn swap_keeping_previous(staged: &Path, target: &Path, previous: &Path) -> Result<Option<PathBuf>> {
    fs::rename(target, previous).map_err(|err| {
        if is_permission_error(&err) {
            cannot_write(target, err)
        } else {
            io_error(target, err)
        }
    })?;
    match fs::rename(staged, target) {
        Ok(()) => Ok(Some(previous.to_path_buf())),
        Err(err) => {
            // Put the original back rather than leave no build at all.
            let restored = fs::rename(previous, target);
            let _ = fs::remove_file(staged);
            if restored.is_err() {
                return Err(Error::Other(anyhow::Error::new(err).context(format!(
                    "the new build could not be put in place and the previous one could not be restored; it is still at {}",
                    previous.display()
                ))));
            }
            if is_permission_error(&err) {
                return Err(cannot_write(target, err));
            }
            Err(io_error(target, err))
        }
    }
}

/// A sidecar path that is not taken: `<target>.<suffix>`, then
/// `<target>.<suffix>.1`, and so on. A previous build the user was told still
/// exists is never written over by the next update.
#[cfg(any(windows, test))]
fn free_sidecar(target: &Path, suffix: &str) -> Result<PathBuf> {
    let name = target
        .file_name()
        .ok_or_else(|| {
            Error::Other(anyhow!(
                "{} is not a file this CLI can replace",
                target.display()
            ))
        })?
        .to_string_lossy()
        .into_owned();
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    for attempt in 0..16 {
        let tail = match attempt {
            0 => String::new(),
            other => format!(".{other}"),
        };
        let candidate = parent.join(format!("{name}.{suffix}{tail}"));
        if fs::symlink_metadata(&candidate).is_err() {
            return Ok(candidate);
        }
    }
    Err(Error::Other(anyhow!(
        "no free name beside {} for the previous build",
        target.display()
    )))
}

/// Whether a filesystem error is "you may not write here" rather than anything
/// else. Only those answers the installer, because only those the installer
/// can fix.
fn is_permission_error(err: &std::io::Error) -> bool {
    matches!(
        err.kind(),
        std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::ReadOnlyFilesystem
    )
}

/// The refusal for a target this CLI cannot write: what happened, and the one
/// command that will install a build there anyway. No elevation is attempted.
fn cannot_write(target: &Path, err: std::io::Error) -> Error {
    Error::Other(anyhow::Error::new(err).context(format!(
        "cannot replace {}; the installer can write there, run:\n  {}",
        target.display(),
        installer_command()
    )))
}

/// Any other filesystem failure while replacing the target.
fn io_error(target: &Path, err: std::io::Error) -> Error {
    Error::Other(anyhow::Error::new(err).context(format!("cannot replace {}", target.display())))
}

/// A filesystem failure anywhere else in the run, named by what it was doing.
fn fs_fail(err: std::io::Error, what: &'static str) -> Error {
    Error::Other(anyhow::Error::new(err).context(what.to_string()))
}

/// The install command for this platform.
fn installer_command() -> &'static str {
    if cfg!(windows) {
        INSTALLER_WINDOWS
    } else {
        INSTALLER_POSIX
    }
}

/// Copy a verified build onto the staged file.
fn copy_file(from: &Path, to: &mut fs::File) -> Result<()> {
    let mut source =
        fs::File::open(from).map_err(|err| fs_fail(err, "cannot read the new build"))?;
    std::io::copy(&mut source, to).map_err(|err| {
        Error::Other(anyhow::Error::new(err).context("cannot write the new build"))
    })?;
    Ok(())
}

/// A private temporary directory, removed on every path out of the run.
struct TempDir(PathBuf);

impl TempDir {
    /// A fresh `0700` directory named after the run, next to the system
    /// temporary directory.
    ///
    /// The mode is part of the `mkdir`, not a `chmod` after it: a directory
    /// that exists for an instant at the umask default is a window another
    /// local user has to notice, and there is nothing to gain from it.
    fn new(label: &str) -> Result<Self> {
        let base = std::env::temp_dir();
        let pid = std::process::id();
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt as _;
            builder.mode(0o700);
        }
        let mut last: Option<std::io::Error> = None;
        for attempt in 0..16 {
            let suffix = match attempt {
                0 => String::new(),
                other => format!("-{other}"),
            };
            let path = base.join(format!("{label}-{pid}{suffix}"));
            match builder.create(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                    last = Some(err);
                    continue;
                }
                Err(err) => {
                    return Err(Error::Other(
                        anyhow::Error::new(err)
                            .context("cannot create a temporary directory for the download"),
                    ));
                }
            }
        }
        Err(Error::Other(
            anyhow::Error::new(
                last.unwrap_or_else(|| std::io::Error::other("no free temporary directory name")),
            )
            .context("cannot create a temporary directory for the download"),
        ))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests;
