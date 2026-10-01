//! The update path, driven end to end against a local release index.
//!
//! Every test here serves its own `127.0.0.1` index, so nothing reaches the
//! network and nothing touches a real binary: the target is always a file in a
//! temporary directory, which is the whole reason the install path takes it as
//! a parameter.

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use semver::Version;
use sha2::{Digest as _, Sha256};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

use super::{Config, Outcome, run};
use crate::error::Error;

/// The cap the archive tests read against. Small on purpose: the cap is a
/// parameter, so a test proves the arithmetic without inflating a quarter of a
/// gigabyte.
const CAP: usize = 4096;

/// An archive under test, and the refusal it has to earn.
type ArchiveCase = (Vec<(&'static str, &'static [u8])>, &'static str);

/// The key the running platform publishes under.
fn key() -> &'static str {
    super::platform_key().expect("this platform is one the release index publishes")
}

/// A `Config` pointed at `url`, running as version `local`.
fn config(url: &str, local: &str) -> Config {
    Config {
        manifest_url: url.to_string(),
        check_only: false,
        local: Version::parse(local).expect("a version under test"),
        request_timeout: Duration::from_secs(30),
    }
}

/// The flag wins, then the environment variable the installers read, then the
/// published address. An empty value is no value.
#[test]
fn the_release_index_address_is_resolved_in_order() {
    for (explicit, from_env, expected) in [
        (
            Some("https://flag.example/latest.json"),
            Some("https://env.example/latest.json"),
            "https://flag.example/latest.json",
        ),
        (
            None,
            Some("https://env.example/latest.json"),
            "https://env.example/latest.json",
        ),
        (Some("  "), Some(""), super::DEFAULT_MANIFEST_URL),
        (None, None, super::DEFAULT_MANIFEST_URL),
    ] {
        assert_eq!(super::resolve_manifest_url(explicit, from_env), expected);
    }
}

/// The SHA-256 of some bytes, as the index spells it.
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// A release index naming `assets` and `digests` for version `version`.
fn index(version: &str, assets: &[(&str, &str)], digests: &[(&str, &str)]) -> String {
    let entries = |pairs: &[(&str, &str)]| {
        pairs
            .iter()
            .map(|(k, v)| format!("{k:?}: {v:?}"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    format!(
        "{{\"version\": {version:?}, \"assets\": {{{}}}, \"sha256\": {{{}}}}}",
        entries(assets),
        entries(digests)
    )
}

/// An HTTP/1.1 answer with a body and a matching `Content-Length`.
fn response(status: &str, content_type: &str, body: &[u8]) -> Vec<u8> {
    let mut out = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    out.extend_from_slice(body);
    out
}

/// Serve a release index and one download on `127.0.0.1`, answering
/// `/latest.json` with `manifest` and every other path with `asset`.
///
/// The index is built per request from the origin it was reached on, so its
/// download URL is on the same host as itself — which is what the origin pin
/// demands. Returns the index URL.
async fn serve<F>(manifest: F, asset: Vec<u8>) -> String
where
    F: Fn(&str) -> String + Send + Sync + 'static,
{
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("a loopback port");
    let addr = listener.local_addr().expect("the bound address");
    let origin = format!("http://{addr}");
    let served = origin.clone();
    let manifest = Arc::new(manifest);
    tokio::spawn(async move {
        let origin = served;
        while let Ok((mut socket, _)) = listener.accept().await {
            let manifest = Arc::clone(&manifest);
            let origin = origin.clone();
            let asset = asset.clone();
            tokio::spawn(async move {
                let mut request = vec![0u8; 4096];
                // One read is enough to see the request line; a partial read
                // simply leaves the test on the download branch.
                let _ = socket.read(&mut request).await;
                let request = String::from_utf8_lossy(&request);
                let target = request.split_whitespace().nth(1).unwrap_or("/");
                let answer = if target.starts_with("/latest.json") {
                    response("200 OK", "application/json", manifest(&origin).as_bytes())
                } else {
                    response("200 OK", "application/octet-stream", &asset)
                };
                let _ = socket.write_all(&answer).await;
                let _ = socket.flush().await;
            });
        }
    });
    format!("{origin}/latest.json")
}

/// A build the index publishes, and the target it would replace.
fn release(directory: &Path) -> (Vec<u8>, PathBuf) {
    let build = b"selfhost 9.9.9, the newest release\n".to_vec();
    let target = directory.join("selfhost");
    fs::write(&target, b"selfhost 0.1.1, the running build\n").expect("a target to replace");
    (build, target)
}

/// Every file in `directory` that the run may have left behind.
fn leftovers(directory: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(directory)
        .expect("the directory is readable")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name != "selfhost")
        .collect();
    names.sort();
    names
}

/// Whether nothing exists at `path` — a symlink counts as existing, which is
/// the point: a planted link is a leftover too.
fn absent(path: &Path) -> bool {
    fs::symlink_metadata(path).is_err()
}

/// The message an error carries, for assertions about wording.
fn message(err: &Error) -> String {
    format!("{err:#}")
}

/// A release whose bytes hash to what the index publishes replaces the build
/// in place, and the new bytes are runnable.
#[tokio::test]
async fn a_verified_release_replaces_the_running_build() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let (build, target) = release(directory.path());
    let key = key();
    let hash = digest(&build);
    let url = serve(
        move |origin| {
            index(
                "9.9.9",
                &[(key, &format!("{origin}/download"))],
                &[(key, &hash)],
            )
        },
        build.clone(),
    )
    .await;

    let outcome = run(&config(&url, "0.1.1"), &target)
        .await
        .expect("a verified release installs");

    assert_eq!(
        outcome,
        Outcome::Installed {
            version: Version::new(9, 9, 9),
            target: target.clone(),
            backup: None,
        }
    );
    assert_eq!(fs::read(&target).expect("the new build is in place"), build);
    assert_eq!(leftovers(directory.path()), Vec::<String>::new());

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = fs::metadata(&target)
            .expect("metadata")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o755, "the new build is runnable");
    }
}

/// Bytes that do not hash to the published value are refused, and the
/// installed build is left exactly as it was.
#[tokio::test]
async fn a_release_that_does_not_match_its_hash_is_refused() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let (build, target) = release(directory.path());
    let before = fs::read(&target).expect("the running build");
    let key = key();
    let url = serve(
        move |origin| {
            index(
                "9.9.9",
                &[(key, &format!("{origin}/download"))],
                &[(key, &"0".repeat(64))],
            )
        },
        build,
    )
    .await;

    let err = run(&config(&url, "0.1.1"), &target)
        .await
        .expect_err("a release that fails its hash never installs");

    assert!(
        message(&err).contains("does not match its published SHA-256"),
        "{err}"
    );
    assert_eq!(
        fs::read(&target).expect("the running build is untouched"),
        before
    );
    assert_eq!(leftovers(directory.path()), Vec::<String>::new());
}

/// The same refusal for a release that arrives in many chunks rather than one:
/// the whole body is streamed and hashed before anything is written next to the
/// target, so a mismatch discovered at the end is as inert as one discovered at
/// the first byte.
#[tokio::test]
async fn a_multi_chunk_release_that_fails_its_hash_leaves_nothing_behind() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let (_, target) = release(directory.path());
    let before = fs::read(&target).expect("the running build");
    // Several times any socket buffer, so this is genuinely a streamed body.
    let build: Vec<u8> = (0..8 * 1024 * 1024u32).map(|i| i as u8).collect();
    let key = key();
    let url = serve(
        move |origin| {
            index(
                "9.9.9",
                &[(key, &format!("{origin}/download"))],
                &[(key, &"0".repeat(64))],
            )
        },
        build,
    )
    .await;

    let err = run(&config(&url, "0.1.1"), &target)
        .await
        .expect_err("a release that fails its hash never installs");

    assert!(
        message(&err).contains("does not match its published SHA-256"),
        "{err}"
    );
    assert_eq!(
        fs::read(&target).expect("the running build is untouched"),
        before,
        "the installed bytes changed"
    );
    assert_eq!(
        leftovers(directory.path()),
        Vec::<String>::new(),
        "a staging file was left beside the target"
    );
}

/// A body past the cap is refused by its declared length before it is read, so
/// an endless answer cannot be buffered and no download is started on the back
/// of a release list that was never a release list.
#[tokio::test]
async fn an_index_past_the_cap_is_refused() {
    let huge = "x".repeat(super::MAX_INDEX_BYTES + 1024);
    let url = serve(move |_| huge.clone(), Vec::new()).await;
    let directory = tempfile::tempdir().expect("a temporary directory");
    let (_, target) = release(directory.path());

    let err = run(&config(&url, "0.1.1"), &target)
        .await
        .expect_err("an index past the cap is refused");

    assert!(message(&err).contains("past the 1 MiB"), "{err}");
    assert_eq!(
        fs::read(&target).expect("the running build is untouched"),
        b"selfhost 0.1.1, the running build\n"
    );
}

/// A download past the cap is refused the same way, naming its own limit.
#[tokio::test]
async fn a_download_past_the_cap_is_refused() {
    let key = key();
    let url = serve(
        move |origin| {
            index(
                "9.9.9",
                &[(key, &format!("{origin}/download"))],
                &[(key, &"0".repeat(64))],
            )
        },
        vec![0u8; super::MAX_ARTIFACT_BYTES + 1],
    )
    .await;
    let directory = tempfile::tempdir().expect("a temporary directory");
    let (_, target) = release(directory.path());

    let err = run(&config(&url, "0.1.1"), &target)
        .await
        .expect_err("a download past the cap is refused");

    assert!(message(&err).contains("past the 256 MiB"), "{err}");
    assert_eq!(
        leftovers(directory.path()),
        Vec::<String>::new(),
        "a staging file was left beside the target"
    );
}

/// A build this CLI may not write is answered with the installer that can,
/// and nothing else is attempted.
#[cfg(unix)]
#[tokio::test]
async fn a_target_that_cannot_be_written_names_the_installer() {
    use std::os::unix::fs::PermissionsExt as _;

    let directory = tempfile::tempdir().expect("a temporary directory");
    let (build, target) = release(directory.path());
    let key = key();
    let hash = digest(&build);
    let url = serve(
        move |origin| {
            index(
                "9.9.9",
                &[(key, &format!("{origin}/download"))],
                &[(key, &hash)],
            )
        },
        build,
    )
    .await;

    // A read-only directory refuses the staged file. A process that writes
    // anyway (a root-owned system install run as root) proves nothing, so the
    // case is skipped rather than asserted against.
    fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o555))
        .expect("the directory is read-only");
    if fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(directory.path().join("probe"))
        .is_ok()
    {
        return;
    }

    let err = run(&config(&url, "0.1.1"), &target)
        .await
        .expect_err("a build that cannot be written is refused");
    fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o755))
        .expect("the directory is writable again");

    let message = message(&err);
    assert!(message.contains("installer can write there"), "{message}");
    assert!(
        message.contains("curl -fsSL https://cli.selfhost.dev/install.sh"),
        "{message}"
    );
    assert_eq!(
        fs::read(&target).expect("the running build is untouched"),
        b"selfhost 0.1.1, the running build\n"
    );
}

/// A local build at the published version, ahead of it, or a pre-release:
/// each is answered in one line and downloads nothing.
#[tokio::test]
async fn versions_are_compared_before_anything_is_downloaded() {
    let key = key();
    let url = serve(
        move |_| {
            index(
                "1.0.0",
                &[(key, "https://127.0.0.1:1/download")],
                &[(key, &"0".repeat(64))],
            )
        },
        b"never downloaded".to_vec(),
    )
    .await;

    for (local, expected) in [
        (
            "1.0.0",
            Outcome::UpToDate {
                latest: Version::new(1, 0, 0),
            },
        ),
        (
            "2.0.0",
            Outcome::LocalIsNewer {
                latest: Version::new(1, 0, 0),
            },
        ),
        (
            "2.0.0-rc.1",
            Outcome::PreRelease {
                latest: Version::new(1, 0, 0),
            },
        ),
    ] {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let (_, target) = release(directory.path());
        let outcome = run(&config(&url, local), &target)
            .await
            .unwrap_or_else(|err| panic!("{local} is answered, not refused: {err}"));
        assert_eq!(outcome, expected, "local {local}");
        assert_eq!(
            fs::read(&target).expect("the running build is untouched"),
            b"selfhost 0.1.1, the running build\n",
            "local {local}"
        );
    }
}

/// `--check` reports both versions and whether an update is waiting, whatever
/// the comparison is, and installs nothing.
#[tokio::test]
async fn check_reports_both_versions_without_installing() {
    let key = key();
    let url = serve(
        move |_| {
            index(
                "3.0.0",
                &[(key, "https://127.0.0.1:1/download")],
                &[(key, &"0".repeat(64))],
            )
        },
        b"never downloaded".to_vec(),
    )
    .await;

    for (local, available) in [("1.0.0", true), ("3.0.0", false), ("3.0.0-rc.1", false)] {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let (_, target) = release(directory.path());
        let mut config = config(&url, local);
        config.check_only = true;
        let outcome = run(&config, &target)
            .await
            .unwrap_or_else(|err| panic!("--check on {local} reports, not refuses: {err}"));
        assert_eq!(
            outcome,
            Outcome::Checked {
                current: Version::parse(local).expect("a version under test"),
                latest: Version::new(3, 0, 0),
                available,
            },
            "local {local}"
        );
        assert_eq!(
            fs::read(&target).expect("the running build is untouched"),
            b"selfhost 0.1.1, the running build\n",
            "local {local}"
        );
    }
}

/// An index that publishes nothing for this platform is refused before any
/// download is attempted.
#[tokio::test]
async fn an_index_without_this_platform_is_refused() {
    let url = serve(
        |_| index("9.9.9", &[("plan9-x86_64", "https://127.0.0.1:1/x")], &[]),
        b"never downloaded".to_vec(),
    )
    .await;
    let directory = tempfile::tempdir().expect("a temporary directory");
    let (_, target) = release(directory.path());

    let err = run(&config(&url, "0.1.1"), &target)
        .await
        .expect_err("a platform with no release is refused");

    assert!(message(&err).contains("no download for"), "{err}");
    assert_eq!(
        fs::read(&target).expect("the running build is untouched"),
        b"selfhost 0.1.1, the running build\n"
    );
}

/// An index whose hash is not 64 hex characters is a malformed index, not a
/// build to install.
#[tokio::test]
async fn a_malformed_hash_is_refused() {
    let key = key();
    let build = b"never installed".to_vec();
    let url = serve(
        move |origin| {
            index(
                "9.9.9",
                &[(key, &format!("{origin}/download"))],
                &[(key, "not-a-hash")],
            )
        },
        build,
    )
    .await;
    let directory = tempfile::tempdir().expect("a temporary directory");
    let (_, target) = release(directory.path());

    let err = run(&config(&url, "0.1.1"), &target)
        .await
        .expect_err("a malformed hash is refused");

    assert!(
        message(&err).contains("does not carry a valid SHA-256"),
        "{err}"
    );
}

/// A download pointed at another site is refused even when the index published
/// a hash for it: the index picks the release, not the place it comes from.
#[tokio::test]
async fn an_off_origin_download_is_refused() {
    let key = key();
    let build = b"never installed".to_vec();
    let hash = digest(&build);
    let url = serve(
        move |_| {
            index(
                "9.9.9",
                &[(key, "https://downloads.example/selfhost")],
                &[(key, &hash)],
            )
        },
        build,
    )
    .await;
    let directory = tempfile::tempdir().expect("a temporary directory");
    let (_, target) = release(directory.path());

    let err = run(&config(&url, "0.1.1"), &target)
        .await
        .expect_err("a download off the index's own site is refused");

    assert!(message(&err).contains("not the same site"), "{err}");
    assert_eq!(
        fs::read(&target).expect("the running build is untouched"),
        b"selfhost 0.1.1, the running build\n"
    );
}

/// The release index itself must be `https` unless it is this machine.
#[tokio::test]
async fn a_plain_http_index_is_refused() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let (_, target) = release(directory.path());

    let err = run(
        &config("http://cli.selfhost.dev/latest.json", "0.1.1"),
        &target,
    )
    .await
    .expect_err("plain http off this machine is refused");

    assert!(message(&err).contains("must use https"), "{err}");
}

/// The download address carries the same rule, and no user info: an address
/// whose user info names localhost while its host does not is exactly the
/// bypass the loopback exemption must not allow.
#[test]
fn a_download_address_is_validated_the_same_way() {
    for (url, ok) in [
        ("https://cli.selfhost.dev/v0.1.1/selfhost", true),
        ("http://127.0.0.1:8080/latest.json", true),
        ("http://localhost:8080/latest.json", true),
        ("http://[::1]:8080/latest.json", true),
        ("http://cli.selfhost.dev/latest.json", false),
        ("ftp://cli.selfhost.dev/latest.json", false),
        ("http://localhost:8080@evil.example/latest.json", false),
        ("http://user:pw@127.0.0.1:8080/latest.json", false),
        ("not a url", false),
    ] {
        let parsed = super::Source::parse(url, "the address");
        assert_eq!(parsed.is_ok(), ok, "{url} should parse: {parsed:?}");
    }
}

/// The pin is on the whole origin, not just the host: the same host on another
/// port is another site, and the same port under another scheme is not the
/// one the index was read from.
#[test]
fn a_download_is_pinned_to_the_whole_origin() {
    let same = |a: &str, b: &str| {
        let a = super::Source::parse(a, "the index address").expect("a usable address");
        let b = super::Source::parse(b, "the download address").expect("a usable address");
        a.origin() == b.origin()
    };

    assert!(same(
        "https://cli.selfhost.dev/latest.json",
        "https://cli.selfhost.dev/v0.1.1/selfhost",
    ));
    assert!(same(
        "https://cli.selfhost.dev/latest.json",
        "https://CLI.SelfHost.dev/v0.1.1/selfhost",
    ));
    // The default port in the address and the default port in force are the
    // same origin.
    assert!(same(
        "https://cli.selfhost.dev/latest.json",
        "https://cli.selfhost.dev:443/v0.1.1/selfhost",
    ));
    assert!(!same(
        "https://cli.selfhost.dev/latest.json",
        "https://cli.selfhost.dev:8443/v0.1.1/selfhost",
    ));
    // The scheme is part of it too: the loopback exemption is what lets plain
    // http be tested at all, and it is still a different site.
    assert!(!same(
        "https://127.0.0.1:8080/latest.json",
        "http://127.0.0.1:8080/download",
    ));
    assert!(!same(
        "https://cli.selfhost.dev/latest.json",
        "https://cdn.example/v0.1.1/selfhost",
    ));
}

/// A refusal names the address without printing the credentials in it — the
/// parser exists to reject a password, not to echo one into a terminal.
#[test]
fn an_address_is_named_without_its_credentials() {
    for (raw, expected) in [
        (
            "http://user:hunter2@127.0.0.1:8080/latest.json",
            "http://127.0.0.1:8080/latest.json",
        ),
        (
            "https://user@hunter2.example/x?a=b",
            "https://hunter2.example/x?a=b",
        ),
        (
            "https://cli.selfhost.dev/v0.1.1/selfhost",
            "https://cli.selfhost.dev/v0.1.1/selfhost",
        ),
        ("not a url", "not a url"),
    ] {
        assert_eq!(super::without_user_info(raw), expected, "{raw}");
    }

    let err = super::Source::parse("https://user:hunter2@cli.selfhost.dev/x", "the address")
        .expect_err("credentials are refused");
    let message = message(&err);
    assert!(message.contains("must not carry a username"), "{message}");
    assert!(!message.contains("hunter2"), "{message}");
}

/// The staging file is created, never adopted: a name that is already taken —
/// by a leftover, or by a symlink planted to catch the write — is stepped over
/// rather than opened.
#[cfg(unix)]
#[test]
fn a_planted_staging_name_is_stepped_over() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let (_, target) = release(directory.path());
    let trap = directory
        .path()
        .join(format!("selfhost.new-{}", std::process::id()));
    let elsewhere = directory.path().join("elsewhere");
    fs::write(&elsewhere, b"not a build").expect("the trap target");
    std::os::unix::fs::symlink(&elsewhere, &trap).expect("a symlink at the staging name");

    let staged = super::stage_next_to(&target).expect("a staging file is created");

    assert_ne!(
        staged.path, trap,
        "the planted name was opened instead of stepped over"
    );
    assert_eq!(
        fs::read(&elsewhere).expect("the trap target"),
        b"not a build",
        "the write went through the planted symlink"
    );
    let staged_path = staged.path.clone();
    drop(staged);
    let _ = fs::remove_file(staged_path);
}

/// The last check before the rename, and the race it closes: the staging name
/// is the pid plus a sweep of sixteen, so anything with write access to the
/// binary's own directory can put a different file there between the write and
/// the swap. The bytes are checked again at that point, and a substitution is
/// refused rather than renamed into place.
#[test]
fn a_staged_file_swapped_out_from_under_the_update_is_refused() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let (_, target) = release(directory.path());
    let installed = fs::read(&target).expect("the running build");
    let build = b"selfhost 9.9.9, the newest release\n".to_vec();
    let verified = digest(&build);

    let mut staged = super::stage_next_to(&target).expect("a staging file is created");
    let mut file = staged.file.take().expect("the staging handle");
    file.write_all(&build).expect("the build is staged");
    file.sync_all().expect("the staging file is flushed");
    drop(file);
    assert!(
        super::verify_staged(&staged, &verified).is_ok(),
        "the staged build verifies before anything is swapped"
    );

    // The attack: a different file at the predictable staging name.
    fs::write(&staged.path, b"a build that was never verified").expect("the substitution");
    let err = super::verify_staged(&staged, &verified)
        .expect_err("a substituted staging file is refused");

    assert!(
        message(&err).contains("changed while the update was running"),
        "{err}"
    );
    assert!(
        absent(&staged.path),
        "the substituted file was left beside the target"
    );
    assert_eq!(
        fs::read(&target).expect("the running build is untouched"),
        installed,
        "the target was replaced with the substituted file"
    );
}

/// The Windows swap, tested here rather than only where it runs: the previous
/// build ends up at the sidecar path the caller is told about.
#[test]
fn a_swap_that_keeps_the_previous_build_leaves_it_at_the_sidecar() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let target = directory.path().join("selfhost.exe");
    let staged = directory.path().join("selfhost.exe.new-1");
    let previous = super::free_sidecar(&target, "old").expect("a free sidecar name");
    fs::write(&target, b"the running build").expect("a build to replace");
    fs::write(&staged, b"the new build").expect("the staged build");

    let kept = super::swap_keeping_previous(&staged, &target, &previous)
        .expect("the swap succeeds")
        .expect("the previous build is reported");

    assert_eq!(kept, previous, "the reported path is the one that was used");
    assert_eq!(
        fs::read(&target).expect("the new build is in place"),
        b"the new build"
    );
    assert_eq!(
        fs::read(&previous).expect("the previous build is kept"),
        b"the running build"
    );
    assert!(!staged.exists(), "the staged build was moved, not copied");
}

/// When the second rename fails there is a moment with no build at the target,
/// so the original goes back where it was.
#[test]
fn a_swap_that_cannot_finish_puts_the_original_back() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let target = directory.path().join("selfhost.exe");
    let previous = super::free_sidecar(&target, "old").expect("a free sidecar name");
    fs::write(&target, b"the running build").expect("a build to replace");
    // A staged path under a directory that does not exist cannot be renamed.
    let staged = directory.path().join("gone").join("selfhost.exe.new-1");

    let err = super::swap_keeping_previous(&staged, &target, &previous)
        .expect_err("a staged build that is not there cannot be put in place");

    assert!(message(&err).contains("cannot replace"), "{err}");
    assert_eq!(
        fs::read(&target).expect("the original is back in place"),
        b"the running build"
    );
    assert!(
        !previous.exists(),
        "the original was left at the sidecar instead of restored"
    );
}

/// A previous build the user was told still exists is never written over: the
/// second update takes the next free sidecar name.
#[test]
fn a_sidecar_name_is_never_taken_twice() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let target = directory.path().join("selfhost.exe");

    let first = super::free_sidecar(&target, "old").expect("a free sidecar name");
    fs::write(&first, b"the build from last time").expect("a kept build");
    let second = super::free_sidecar(&target, "old").expect("another free sidecar name");

    assert_ne!(first, second, "the kept build would be overwritten");
    assert_eq!(
        fs::read(&first).expect("the kept build is untouched"),
        b"the build from last time"
    );
}

/// A 3xx is answered rather than followed, so a moved index cannot walk the
/// download off the site it was read from.
#[tokio::test]
async fn a_redirect_is_not_followed() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("a loopback port");
    let addr = listener.local_addr().expect("the bound address");
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            let mut request = vec![0u8; 4096];
            let _ = socket.read(&mut request).await;
            let answer = response(
                "302 Found",
                "text/plain",
                b"http://evil.example/latest.json".as_slice(),
            );
            let _ = socket.write_all(&answer).await;
            let _ = socket.flush().await;
        }
    });

    let directory = tempfile::tempdir().expect("a temporary directory");
    let (_, target) = release(directory.path());
    let err = run(
        &config(&format!("http://{addr}/latest.json"), "0.1.1"),
        &target,
    )
    .await
    .expect_err("a redirect is refused");

    assert!(message(&err).contains("302"), "{err}");
    assert!(message(&err).contains("not followed"), "{err}");
}

/// A zip holding one `selfhost.exe` at its root yields exactly those bytes.
#[test]
fn a_root_executable_is_taken_out_of_the_archive() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let archive = directory.path().join("release.zip");
    let payload = b"MZ the executable".to_vec();
    fs::write(
        &archive,
        zip_of(&[("selfhost.exe", &payload)]).expect("an archive is written"),
    )
    .expect("the archive is on disk");

    let out = directory.path().join("selfhost.exe");
    super::extract_executable(&archive, &out, CAP).expect("the archive is readable");

    assert_eq!(fs::read(&out).expect("the executable"), payload);
}

/// An entry whose name carries a separator is refused as naming something
/// inside the archive that is not at its root.
#[test]
fn an_entry_with_a_separator_in_its_name_is_refused() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    for name in ["nested/selfhost.exe", "..\\evil"] {
        let archive = directory.path().join("release.zip");
        fs::write(
            &archive,
            zip_of(&[(name, b"payload")]).expect("an archive is written"),
        )
        .expect("the archive is on disk");

        let err = super::extract_executable(&archive, &directory.path().join("out"), CAP)
            .expect_err("an entry inside the archive is refused");

        assert!(message(&err).contains("outside its root"), "{name}: {err}");
        assert!(
            absent(&directory.path().join("out")),
            "{name}: something was written anyway"
        );
    }
}

/// A name that climbs out of the directory it would be written to is refused
/// on its own account — `..` carries no separator, so the check above cannot
/// see it, and dropping either check has to fail a test.
#[test]
fn an_entry_that_climbs_out_of_the_archive_is_refused() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    // `..` is the only spelling that reaches this check: every other way of
    // climbing out carries a separator and is refused as one.
    let archive = directory.path().join("release.zip");
    fs::write(
        &archive,
        zip_of(&[("..", b"payload")]).expect("an archive is written"),
    )
    .expect("the archive is on disk");

    let err = super::extract_executable(&archive, &directory.path().join("out"), CAP)
        .expect_err("a climbing entry is refused");

    assert!(message(&err).contains("climbs out of it"), "{err}");
    assert!(
        absent(&directory.path().join("out")),
        "something was written anyway"
    );
}

/// `../evil` is both a separator and a climb; whichever check is meant to
/// catch it, it never reaches the filesystem.
#[test]
fn an_entry_that_escapes_the_archive_is_refused() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let archive = directory.path().join("release.zip");
    fs::write(
        &archive,
        zip_of(&[("../evil", b"payload")]).expect("an archive is written"),
    )
    .expect("the archive is on disk");

    let err = super::extract_executable(&archive, &directory.path().join("out"), CAP)
        .expect_err("an escaping entry is refused");

    assert!(message(&err).contains("outside its root"), "{err}");
    assert!(absent(&directory.path().join("out")));
    assert!(absent(&directory.path().parent().unwrap().join("evil")));
}

/// An archive that is not the published one — the wrong entry, or more than
/// the single executable the release promises — is refused.
#[test]
fn an_archive_without_the_executable_is_refused() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    // Each case is an archive and the refusal it has to earn.
    let cases: [ArchiveCase; 2] = [
        (
            vec![("readme.txt", b"nothing to run")],
            "does not contain selfhost.exe",
        ),
        (
            vec![
                ("selfhost.exe", b"the executable"),
                ("readme.txt", b"extra"),
            ],
            "exactly one",
        ),
    ];
    for (entries, expected) in cases {
        let archive = directory.path().join("release.zip");
        fs::write(&archive, zip_of(&entries).expect("an archive is written"))
            .expect("the archive is on disk");

        let err = super::extract_executable(&archive, &directory.path().join("out"), CAP)
            .expect_err("an unexpected archive is refused");

        assert!(message(&err).contains(expected), "{err}");
    }
}

/// An entry that declares more than the cap is refused before a single byte is
/// written: the size is in the archive, so there is nothing to read first.
#[test]
fn an_entry_that_declares_too_much_is_refused_before_anything_is_written() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let archive = directory.path().join("release.zip");
    let declared = (CAP as u64 + 1) as u32;
    fs::write(
        &archive,
        zip_declaring(&[("selfhost.exe", b"MZ")], declared),
    )
    .expect("the archive is on disk");
    let out = directory.path().join("out");

    let err = super::extract_executable(&archive, &out, CAP)
        .expect_err("an entry past the cap is refused");

    assert!(message(&err).contains("past the 4096 bytes"), "{err}");
    assert!(absent(&out), "something was written anyway");
}

/// The size an entry declares is not trusted on its own. Here it is rewritten
/// down to something small, so the declared check passes and the entry is
/// stopped mid-stream instead — which is the only thing standing between a
/// tiny archive and an unbounded file on disk.
#[test]
fn an_entry_that_inflates_past_the_cap_is_refused() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let archive = directory.path().join("release.zip");
    // Ten times the cap of bytes that compress to a fraction of it: the archive
    // is tiny, the entry is not.
    let bomb: Vec<u8> = vec![b'M'; CAP * 10];
    fs::write(&archive, zip_declaring(&[("selfhost.exe", &bomb)], 12))
        .expect("the archive is on disk");
    let out = directory.path().join("out");

    let err = super::extract_executable(&archive, &out, CAP)
        .expect_err("an entry that inflates past the cap is refused");

    assert!(message(&err).contains("past the 4096 bytes"), "{err}");
    let written = fs::metadata(&out).map(|meta| meta.len()).unwrap_or(0);
    assert!(
        written <= CAP as u64,
        "more than the cap reached the disk: {written} bytes"
    );
}

/// A release archive, built in memory with the same crate that reads it, then
/// with the uncompressed size the central directory advertises rewritten.
fn zip_declaring(entries: &[(&str, &[u8])], declared: u32) -> Vec<u8> {
    let mut bytes = zip_of(entries).expect("an archive is written");
    let central = bytes
        .windows(4)
        .position(|window| window == b"PK\x01\x02")
        .expect("the central directory is in there");
    // A central directory header carries the uncompressed size at offset 24.
    bytes[central + 24..central + 28].copy_from_slice(&declared.to_le_bytes());
    bytes
}

/// A release archive, built in memory with the same crate that reads it.
fn zip_of(entries: &[(&str, &[u8])]) -> Result<Vec<u8>, zip::result::ZipError> {
    use std::io::Write as _;

    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    for (name, body) in entries {
        writer.start_file(*name, options)?;
        writer.write_all(body)?;
    }
    Ok(writer.finish()?.into_inner())
}
