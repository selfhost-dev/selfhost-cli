//! Loopback sign-in listener (design §5).
//!
//! Binds `127.0.0.1:0` (never `0.0.0.0`), opens
//! `${console_url}/mcp-auth?port=N`, and waits for the console to redirect the
//! browser to `GET /callback?refresh_token=…&api_key=…`.
//!
//! **No CSRF `state` parameter.** Design §5c asks for one, but the shipped
//! console page builds the callback URL itself and never echoes extra query
//! parameters, so a `state` this CLI invented could never come back to be
//! checked — the check would be theatre. What actually protects the hop is the
//! loopback-only bind, an ephemeral port, the two-minute window, and the
//! requirement that the callback carry both parameters.

use std::io::IsTerminal;
use std::net::Ipv4Addr;
use std::time::Duration;

use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, Lines, Stdin};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

use crate::error::{Error, Result};

use super::Credentials;

/// Bytes read from one connection before its request is abandoned.
const MAX_REQUEST_BYTES: usize = 16 * 1024;
/// A connection must deliver its request line within this window.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
/// How long [`login`] waits for the browser before giving up (design §5).
const LOGIN_TIMEOUT: Duration = Duration::from_secs(120);

const SUCCESS_HTML: &str =
    "<h1>Authorization complete!</h1><p>You can close this tab and return to the terminal.</p>";
const MISSING_PARAMS_HTML: &str =
    "<h1>Missing parameters</h1><p>refresh_token and api_key are required.</p>";
const NOT_FOUND_HTML: &str = "<h1>Not found</h1>";

/// The credential pair a `/callback` request carried.
struct Callback {
    api_key: String,
    refresh_token: String,
}

/// The console URL the browser is sent to for an ephemeral loopback `port`.
pub(crate) fn auth_url(console_url: &str, port: u16) -> String {
    format!("{}/mcp-auth?port={port}", console_url.trim_end_matches('/'))
}

/// Run the loopback handshake and return the credentials the console handed back.
pub(crate) async fn login(console_url: &str, no_browser: bool) -> Result<Credentials> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.map_err(|err| {
        Error::Other(
            anyhow::Error::from(err).context("cannot listen for the sign-in callback on 127.0.0.1"),
        )
    })?;
    let port = listener
        .local_addr()
        .map_err(|err| Error::Other(anyhow::Error::from(err).context("cannot read the callback port")))?
        .port();

    announce(&auth_url(console_url, port), port, no_browser);

    await_callback(listener, no_browser).await
}

/// Wait for the `/callback` request — or a pasted redirect — on a bound loopback
/// `listener`, giving up after [`LOGIN_TIMEOUT`].
async fn await_callback(listener: TcpListener, no_browser: bool) -> Result<Credentials> {
    let (sender, mut receiver) = mpsc::unbounded_channel::<Callback>();
    let deadline = tokio::time::sleep(LOGIN_TIMEOUT);
    tokio::pin!(deadline);

    // Only `--no-browser` accepts a pasted redirect; with a browser the user
    // never sees a URL to copy.
    let mut pasted = no_browser.then(PasteReader::new);
    let mut pasted_open = pasted.is_some();

    loop {
        tokio::select! {
            () = &mut deadline => return Err(timed_out()),

            callback = receiver.recv() => match callback {
                Some(callback) => {
                    return Ok(Credentials {
                        api_key: callback.api_key,
                        refresh_token: callback.refresh_token,
                    });
                }
                None => return Err(timed_out()),
            },

            accepted = listener.accept() => {
                if let Ok((stream, _peer)) = accepted {
                    tokio::spawn(handle_connection(stream, sender.clone()));
                }
            }

            line = next_line(&mut pasted), if pasted_open => match line {
                Ok(PasteLine::Line(line)) => {
                    if let Some(callback) = parse_pasted(&line) {
                        return Ok(Credentials {
                            api_key: callback.api_key,
                            refresh_token: callback.refresh_token,
                        });
                    }
                }
                // Ctrl-C in raw mode is ours to handle: abort the sign-in.
                Ok(PasteLine::Interrupted) => {
                    return Err(Error::Other(anyhow::anyhow!(
                        "sign-in cancelled; run selfhost auth login to try again"
                    )));
                }
                Ok(PasteLine::Ended) | Err(_) => pasted_open = false,
            },
        }
    }
}

/// The exact message a timed-out sign-in produces (design §5: two minutes).
fn timed_out() -> Error {
    Error::Other(anyhow::anyhow!(
        "browser login timed out after 2 minutes; run selfhost auth login to try again"
    ))
}

/// Tell the user where to go. A browser that refuses to open is not fatal.
fn announce(url: &str, port: u16, no_browser: bool) {
    if no_browser {
        eprintln!("Open this URL in a browser to finish signing in:");
        eprintln!("{url}");
        eprintln!(
            "On a remote machine, forward the callback port first: ssh -L {port}:127.0.0.1:{port} user@host"
        );
        eprintln!("Or paste the URL your browser was redirected to here.");
    } else if let Err(err) = open::that(url) {
        eprintln!("Could not open a browser ({err}); open this URL to finish signing in:");
        eprintln!("{url}");
    }
}

/// Serve one callback connection. Browsers also preconnect with nothing to
/// say, so an empty or timed-out read is ignored rather than treated as the
/// answer.
async fn handle_connection(mut stream: TcpStream, sender: mpsc::UnboundedSender<Callback>) {
    let request = match tokio::time::timeout(REQUEST_TIMEOUT, read_request(&mut stream)).await {
        Ok(Ok(request)) => request,
        _ => return,
    };
    let target = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or_default();
    let (path, query) = target_path_and_query(target);

    if is_callback_path(path) {
        match parse_callback_query(query) {
            Some(callback) => {
                respond(&mut stream, 200, SUCCESS_HTML).await;
                let _ = sender.send(callback);
            }
            None => respond(&mut stream, 400, MISSING_PARAMS_HTML).await,
        }
    } else {
        respond(&mut stream, 404, NOT_FOUND_HTML).await;
    }
}

/// Path and query of a request target. An absolute-form target
/// (`GET http://127.0.0.1:PORT/callback?… HTTP/1.1`) has its scheme and
/// authority stripped, so it resolves like an origin-form one.
fn target_path_and_query(target: &str) -> (&str, &str) {
    let path_and_query = match target.split_once("://") {
        Some((_, rest)) => match rest.find('/') {
            Some(slash) => &rest[slash..],
            None => "/",
        },
        None => target,
    };
    path_and_query.split_once('?').unwrap_or((path_and_query, ""))
}

/// Read at most [`MAX_REQUEST_BYTES`] of one request. Only the request line is
/// ever inspected; the cap stops a local process from feeding us megabytes.
async fn read_request(stream: &mut TcpStream) -> std::io::Result<String> {
    let mut buffer = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];
    loop {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            break;
        }
        let room = MAX_REQUEST_BYTES - buffer.len();
        if room == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read.min(room)]);
        if buffer.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
    }
    Ok(String::from_utf8_lossy(&buffer).into_owned())
}

/// Answer one request and close the connection.
async fn respond(stream: &mut TcpStream, status: u16, body: &str) {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        _ => "Not Found",
    };
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\n\
         Content-Type: text/html; charset=utf-8\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.flush().await;
    let _ = stream.shutdown().await;
}

/// One line of pasted input, or why there is none.
enum PasteLine {
    /// A complete line (no trailing newline).
    Line(String),
    /// End of input.
    Ended,
    /// Ctrl-C, which raw mode no longer turns into a signal.
    Interrupted,
}

// Raw mode is held for at most the login window (`LOGIN_TIMEOUT`, two
// minutes). In that window `0x03` is a plain byte, not SIGINT, so
// `read_raw_line` decodes Ctrl-C by hand into `PasteLine::Interrupted`; the
// guard restores the previous terminal mode on every exit path, unwind
// included.
/// Restores the terminal's previous mode when dropped, so echo comes back on
/// every exit path — including an early return and an unwind.
struct RawModeGuard;

impl RawModeGuard {
    /// `Some` when raw mode could be enabled. crossterm restores the mode it
    /// saved when [`Self`] is dropped.
    fn enable() -> Option<Self> {
        enable_raw_mode().ok().map(|()| Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
    }
}

/// Reads the redirect a user pastes at the terminal.
///
/// On a TTY it reads with terminal echo off (crossterm raw mode) so the
/// credential-bearing URL never appears on screen; the previous mode is
/// restored when this is dropped. Without a TTY — or when raw mode cannot be
/// enabled — it falls back to an ordinary line read.
struct PasteReader {
    lines: Option<Lines<BufReader<Stdin>>>,
    _raw: Option<RawModeGuard>,
}

impl PasteReader {
    fn new() -> Self {
        if std::io::stdin().is_terminal()
            && let Some(guard) = RawModeGuard::enable()
        {
            return Self {
                lines: None,
                _raw: Some(guard),
            };
        }
        Self {
            lines: Some(BufReader::new(tokio::io::stdin()).lines()),
            _raw: None,
        }
    }

    /// The next pasted line.
    async fn next_line(&mut self) -> std::io::Result<PasteLine> {
        match &mut self.lines {
            Some(lines) => Ok(match lines.next_line().await? {
                Some(line) => PasteLine::Line(line),
                None => PasteLine::Ended,
            }),
            None => read_raw_line().await,
        }
    }
}

/// One unbuffered, un-echoed line from the terminal. Backspace edits; other
/// control bytes (including the line terminators) are dropped.
async fn read_raw_line() -> std::io::Result<PasteLine> {
    let mut stdin = tokio::io::stdin();
    let mut bytes = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        if stdin.read(&mut byte).await? == 0 {
            return Ok(PasteLine::Ended);
        }
        match byte[0] {
            b'\r' | b'\n' => {
                return Ok(PasteLine::Line(
                    String::from_utf8_lossy(&bytes).into_owned(),
                ));
            }
            0x03 => return Ok(PasteLine::Interrupted),
            0x08 | 0x7f => {
                bytes.pop();
            }
            b if b >= 0x20 => bytes.push(b),
            _ => {}
        }
    }
}

/// [`PasteReader::next_line`] over an optional reader, or a future that never
/// resolves when this sign-in is not reading the terminal.
async fn next_line(pasted: &mut Option<PasteReader>) -> std::io::Result<PasteLine> {
    match pasted {
        Some(reader) => reader.next_line().await,
        None => std::future::pending().await,
    }
}

/// The `refresh_token`/`api_key` pair in a callback query string (no leading
/// `?`), when both are present and non-empty.
fn parse_callback_query(query: &str) -> Option<Callback> {
    let mut api_key = None;
    let mut refresh_token = None;
    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        let value = percent_decode(value);
        if value.is_empty() {
            continue;
        }
        match key {
            "api_key" => api_key = Some(value),
            "refresh_token" => refresh_token = Some(value),
            _ => {}
        }
    }
    Some(Callback {
        api_key: api_key?,
        refresh_token: refresh_token?,
    })
}

/// Parse what a `--no-browser` user pasted: the full redirect URL, its
/// `/callback` path, or just the query string.
fn parse_pasted(input: &str) -> Option<Callback> {
    let input = input.trim();
    if input.is_empty() || input.contains('\n') {
        return None;
    }
    let query = match input.split_once('?') {
        Some((prefix, query)) => {
            if !prefix.is_empty() && !is_callback_path(prefix) {
                return None;
            }
            query
        }
        None => {
            if input.contains("://") {
                return None;
            }
            input
        }
    };
    parse_callback_query(query)
}

/// Whether a pasted URL's part before `?` addresses `/callback`.
fn is_callback_path(prefix: &str) -> bool {
    let path = match prefix.split_once("://") {
        Some((_, rest)) => match rest.find('/') {
            Some(slash) => &rest[slash..],
            None => return false,
        },
        None => prefix,
    };
    path.trim_end_matches('/') == "/callback"
}

/// Decode `%XX` escapes only. `+` stays a literal plus: Firebase tokens and
/// API keys use `+` as data, and the console does not form-encode either.
fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let (Some(high), Some(low)) = (hex_value(bytes[index + 1]), hex_value(bytes[index + 2]))
        {
            out.push(high * 16 + low);
            index += 3;
            continue;
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_in_url_drops_a_trailing_slash() {
        assert_eq!(
            auth_url("https://console.selfhost.dev", 41234),
            "https://console.selfhost.dev/mcp-auth?port=41234"
        );
        assert_eq!(
            auth_url("https://console.selfhost.dev//", 41234),
            "https://console.selfhost.dev/mcp-auth?port=41234"
        );
    }

    #[test]
    fn callback_query_requires_both_parameters() {
        let callback = parse_callback_query("refresh_token=AMf-1&api_key=AIza-2").unwrap();
        assert_eq!(callback.refresh_token, "AMf-1");
        assert_eq!(callback.api_key, "AIza-2");

        assert!(parse_callback_query("refresh_token=AMf-1").is_none());
        assert!(parse_callback_query("api_key=AIza-2").is_none());
        assert!(parse_callback_query("refresh_token=&api_key=AIza-2").is_none());
        assert!(parse_callback_query("").is_none());
        assert!(parse_callback_query("what=ever&other=thing").is_none());
        assert!(parse_callback_query("not-even-a-query").is_none());
    }

    #[test]
    fn callback_query_percent_decodes_escapes_and_keeps_plus_literal() {
        let callback =
            parse_callback_query("refresh_token=AMf%2D1%2Fa%20b&api_key=AIza%3Dx+7").unwrap();
        assert_eq!(callback.refresh_token, "AMf-1/a b");
        assert_eq!(callback.api_key, "AIza=x+7");

        // A stray escape is left as written instead of swallowing the value.
        let callback = parse_callback_query("refresh_token=AMf%zz&api_key=AIza").unwrap();
        assert_eq!(callback.refresh_token, "AMf%zz");
    }

    #[test]
    fn pasted_redirect_is_read_from_a_url_path_or_bare_query() {
        let callback = parse_pasted("  http://localhost:41234/callback?refresh_token=AMf-1&api_key=AIza-2  ")
            .expect("a full redirect URL must parse");
        assert_eq!(callback.refresh_token, "AMf-1");
        assert_eq!(callback.api_key, "AIza-2");

        let callback = parse_pasted("/callback?refresh_token=AMf-1&api_key=AIza-2").unwrap();
        assert_eq!(callback.api_key, "AIza-2");

        let callback = parse_pasted("refresh_token=AMf-1&api_key=AIza-2").unwrap();
        assert_eq!(callback.api_key, "AIza-2");

        assert!(parse_pasted("http://localhost:41234/mcp-auth?port=41234").is_none());
        assert!(parse_pasted("https://console.selfhost.dev").is_none());
        assert!(parse_pasted("not a url at all").is_none());
        assert!(parse_pasted("").is_none());
    }

    #[test]
    fn target_path_and_query_strips_an_absolute_form_target() {
        assert_eq!(
            target_path_and_query("/callback?refresh_token=x"),
            ("/callback", "refresh_token=x")
        );
        assert_eq!(
            target_path_and_query("http://127.0.0.1:41234/callback?api_key=y"),
            ("/callback", "api_key=y")
        );
        assert_eq!(
            target_path_and_query("http://127.0.0.1:41234/callback"),
            ("/callback", "")
        );
        assert_eq!(target_path_and_query("/callback"), ("/callback", ""));
    }

    #[tokio::test]
    async fn absolute_form_callback_is_served_from_a_raw_tcp_client() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (sender, mut receiver) = mpsc::unbounded_channel();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            handle_connection(stream, sender).await;
        });

        let mut client = TcpStream::connect(addr).await.unwrap();
        let request = format!(
            "GET http://127.0.0.1:{}/callback?refresh_token=AMf-1&api_key=AIza-2 HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n",
            addr.port()
        );
        client.write_all(request.as_bytes()).await.unwrap();
        let mut response = Vec::new();
        client.read_to_end(&mut response).await.unwrap();
        let response = String::from_utf8_lossy(&response);
        assert!(response.contains("Authorization complete"), "{response}");

        let callback = receiver.recv().await.expect("the callback is delivered");
        assert_eq!(callback.api_key, "AIza-2");
        assert_eq!(callback.refresh_token, "AMf-1");
        server.await.unwrap();
    }
}
