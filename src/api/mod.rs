//! HTTP client for the platform API: bearer auth, the
//! `{status, data, message, status_code}` Rails envelope, `organization_id`
//! injection (query for GET, body for writes), `Retry-After` handling and the
//! `verbose`/`debug` request echo (design §6).
//!
//! Resource modules (`postgres`, `org`, …) are thin typed wrappers over
//! [`ApiClient`]; nothing else in the CLI opens a socket against the platform.
//! Every response body is parsed as the envelope, so a 2xx that still reports
//! `status: "error"` is an error here — the API's documented contract.

use std::time::Duration;

use reqwest::Method;
use reqwest::header::{ACCEPT, CONTENT_TYPE, HeaderMap, RETRY_AFTER};
use serde_json::Value;

use crate::error::{Error, Result};

/// Ceiling on a single `Retry-After` sleep, so a broken or hostile server
/// cannot park the CLI for hours (design §6).
const MAX_RETRY_AFTER_SECS: u64 = 60;

/// Used when `Retry-After` is absent or is not an integer second count.
const DEFAULT_RETRY_AFTER_SECS: u64 = 1;

/// Longest text snippet quoted from a non-JSON error body, in characters.
const ERROR_SNIPPET_CHARS: usize = 160;

/// Substrings that mark a body key as secret before a `--debug` echo. A key is
/// masked when it contains any of them, case-insensitively.
const SECRET_KEY_MARKERS: [&str; 7] = [
    "password",
    "secret",
    "token",
    "api_key",
    "apikey",
    "private_key",
    "credential",
];

/// Whether a body key names credential material that must never be echoed.
fn is_secret_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    SECRET_KEY_MARKERS.iter().any(|marker| key.contains(marker))
}

// Slice 1 staging: the resource modules that call `ApiClient` land after the
// auth slice, so until then nothing in the binary references it.
#[allow(dead_code)]
/// Authenticated JSON client over the platform API.
pub struct ApiClient {
    http: reqwest::Client,
    base_url: String,
    token: String,
    org: Option<String>,
    verbose: bool,
    debug: bool,
}

#[allow(dead_code)]
impl ApiClient {
    /// Build a client for `base_url` (trailing slash trimmed) that sends
    /// `Bearer <token>` on every request and gives up after `timeout_secs`.
    /// Organization injection is off until [`Self::with_org`].
    pub fn with_token(base_url: &str, token: String, timeout_secs: u64) -> Self {
        // `Client::builder().build()` only fails when the TLS backend refuses
        // to initialize; fall back to a default client rather than panicking.
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
            token,
            org: None,
            verbose: false,
            debug: false,
        }
    }

    /// Attach the organization id injected into scoped requests.
    pub fn with_org(mut self, org: Option<String>) -> Self {
        self.org = org;
        self
    }

    /// `--verbose` echoes `→ METHOD path`; `--debug` also echoes the headers
    /// and the request body (redacted).
    pub fn with_verbosity(mut self, verbose: bool, debug: bool) -> Self {
        self.verbose = verbose;
        self.debug = debug;
        self
    }

    /// `GET path?…` with `organization_id` appended when an org is set and the
    /// caller did not pass one explicitly.
    pub async fn get(&self, path: &str, query: &[(&str, &str)]) -> Result<Value> {
        let query = self.scoped_query(query);
        self.request(Method::GET, path, &query, None).await
    }

    /// `POST path` with `organization_id` inserted into an object body.
    pub async fn post(&self, path: &str, body: Value) -> Result<Value> {
        let body = self.scoped_body(body);
        self.request(Method::POST, path, &[], Some(&body)).await
    }

    /// [`Self::get`] without organization injection (identity endpoints).
    pub async fn get_unscoped(&self, path: &str, query: &[(&str, &str)]) -> Result<Value> {
        self.request(Method::GET, path, query, None).await
    }

    /// [`Self::post`] without organization injection (identity endpoints).
    pub async fn post_unscoped(&self, path: &str, body: Value) -> Result<Value> {
        self.request(Method::POST, path, &[], Some(&body)).await
    }

    /// `PUT path` with `organization_id` inserted into an object body.
    pub async fn put(&self, path: &str, body: Value) -> Result<Value> {
        let body = self.scoped_body(body);
        self.request(Method::PUT, path, &[], Some(&body)).await
    }

    /// `PATCH path` with `organization_id` inserted into an object body.
    pub async fn patch(&self, path: &str, body: Value) -> Result<Value> {
        let body = self.scoped_body(body);
        self.request(Method::PATCH, path, &[], Some(&body)).await
    }

    /// `DELETE path` with a JSON body; the empty case is an empty object. The
    /// body goes through [`Self::scoped_body`] like every other write, so the
    /// routes that resolve their organization from the body still work.
    pub async fn delete(&self, path: &str, body: Value) -> Result<Value> {
        let body = self.scoped_body(body);
        self.request(Method::DELETE, path, &[], Some(&body)).await
    }

    /// Append the organization id unless the caller already supplied one.
    fn scoped_query<'a>(&'a self, query: &'a [(&'a str, &'a str)]) -> Vec<(&'a str, &'a str)> {
        let mut out = query.to_vec();
        if let Some(org) = self.org.as_deref()
            && !out.iter().any(|(key, _)| *key == "organization_id")
        {
            out.push(("organization_id", org));
        }
        out
    }

    /// Insert the organization id into an object body; non-object bodies pass
    /// through untouched (arrays cannot carry a key).
    fn scoped_body(&self, mut body: Value) -> Value {
        if let (Some(org), Value::Object(map)) = (self.org.as_deref(), &mut body)
            && !map.contains_key("organization_id")
        {
            map.insert(
                "organization_id".to_string(),
                Value::String(org.to_string()),
            );
        }
        body
    }

    /// Absolute request URL, with the query pairs percent-encoded by `Url`.
    fn build_url(&self, path: &str, query: &[(&str, &str)]) -> Result<reqwest::Url> {
        let mut url = reqwest::Url::parse(&format!(
            "{}/{}",
            self.base_url,
            path.trim_start_matches('/')
        ))
        .map_err(|err| Error::Other(anyhow::anyhow!("invalid request URL for {path}: {err}")))?;
        if !query.is_empty() {
            let mut pairs = url.query_pairs_mut();
            for (key, value) in query {
                pairs.append_pair(key, value);
            }
        }
        Ok(url)
    }

    /// One logical API call: send, honor a single 429 `Retry-After`, classify.
    async fn request(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, &str)],
        body: Option<&Value>,
    ) -> Result<Value> {
        // `RequestBuilder::query` needs reqwest's `query` feature, which the
        // crate does not enable; build the URL with `Url`'s own encoder so
        // values are percent-encoded exactly once.
        let url = self.build_url(path, query)?;
        let mut retried = false;
        loop {
            self.echo(&method, path, body);
            let mut request = self
                .http
                .request(method.clone(), url.clone())
                .bearer_auth(&self.token)
                .header(ACCEPT, "application/json");
            if let Some(body) = body {
                request = request.header(CONTENT_TYPE, "application/json").json(body);
            }

            let response = request
                .send()
                .await
                .map_err(|err| transport_error(path, err))?;
            let status = response.status();
            let retry_after = retry_after_secs(response.headers());
            let text = response
                .text()
                .await
                .map_err(|err| transport_error(path, err))?;

            // A 429 is slept off and replayed exactly once; a second 429 (or a
            // 429 that arrives after the replay) surfaces as `RateLimited`.
            if status.as_u16() == 429 && !retried {
                retried = true;
                tokio::time::sleep(Duration::from_secs(retry_after)).await;
                continue;
            }
            return classify(status, &text);
        }
    }

    /// `--verbose`/`--debug` request echo, to stderr.
    fn echo(&self, method: &Method, path: &str, body: Option<&Value>) {
        for line in self.echo_lines(method, path, body) {
            eprintln!("{line}");
        }
    }

    /// The echo lines themselves. Never contains the bearer token or query
    /// values; the debug body is redacted with [`redact_value`].
    fn echo_lines(&self, method: &Method, path: &str, body: Option<&Value>) -> Vec<String> {
        if !self.verbose && !self.debug {
            return Vec::new();
        }
        let mut lines = vec![format!("→ {method} {path}")];
        if !self.debug {
            return lines;
        }
        lines.push("  Authorization: Bearer [REDACTED]".to_string());
        lines.push("  Accept: application/json".to_string());
        if let Some(body) = body {
            lines.push("  Content-Type: application/json".to_string());
            let mut redacted = body.clone();
            redact_value(&mut redacted);
            lines.push(match serde_json::to_string_pretty(&redacted) {
                Ok(json) => json,
                Err(_) => "  <unprintable body>".to_string(),
            });
        }
        lines
    }
}

/// Turn the envelope into `data` or the mapped error (design §6).
fn classify(status: reqwest::StatusCode, body: &str) -> Result<Value> {
    let envelope: Option<Value> = serde_json::from_str(body).ok();
    let envelope_status = envelope
        .as_ref()
        .and_then(|value| value.get("status"))
        .and_then(Value::as_str);

    if status.is_success() && envelope_status != Some("error") {
        return Ok(envelope
            .and_then(|value| value.get("data").cloned())
            .unwrap_or(Value::Null));
    }

    let message = envelope
        .as_ref()
        .and_then(|value| value.get("message"))
        .and_then(Value::as_str)
        .filter(|message| !message.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| snippet(status, body));

    match status.as_u16() {
        401 => Err(Error::NotAuthenticated(message)),
        402 => Err(Error::BillingRequired(message)),
        429 => Err(Error::RateLimited(message)),
        code => Err(Error::Other(anyhow::anyhow!("{message} (HTTP {code})"))),
    }
}

/// Message used when the error body carries no `message`: the HTTP status plus
/// a one-line, length-capped snippet of whatever the server sent.
fn snippet(status: reqwest::StatusCode, body: &str) -> String {
    let code = status.as_u16();
    let collapsed = body.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return format!("HTTP {code}");
    }
    let mut snippet: String = collapsed.chars().take(ERROR_SNIPPET_CHARS).collect();
    if collapsed.chars().count() > ERROR_SNIPPET_CHARS {
        snippet.push('…');
    }
    format!("HTTP {code}: {snippet}")
}

/// `Retry-After` in seconds: integer values capped at [`MAX_RETRY_AFTER_SECS`],
/// everything else (absent, garbage, HTTP-date) falls back to
/// [`DEFAULT_RETRY_AFTER_SECS`].
fn retry_after_secs(headers: &HeaderMap) -> u64 {
    parse_retry_after(
        headers
            .get(RETRY_AFTER)
            .and_then(|value| value.to_str().ok()),
    )
}

/// [`retry_after_secs`] without the header lookup, for direct testing.
fn parse_retry_after(raw: Option<&str>) -> u64 {
    raw.and_then(|raw| raw.trim().parse::<u64>().ok())
        .map(|secs| secs.min(MAX_RETRY_AFTER_SECS))
        .unwrap_or(DEFAULT_RETRY_AFTER_SECS)
}

/// Replace the values of obvious secret keys, recursively.
fn redact_value(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, value) in map.iter_mut() {
                if is_secret_key(key) {
                    *value = Value::String("[REDACTED]".to_string());
                } else {
                    redact_value(value);
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(redact_value),
        _ => {}
    }
}

/// Transport-level failure, normalized to `Other`. `path` is echoed instead of
/// the full URL and the URL is stripped from the source error so no query
/// values leak.
fn transport_error(path: &str, err: reqwest::Error) -> Error {
    let hint = if err.is_timeout() { " (timed out)" } else { "" };
    Error::Other(anyhow::anyhow!(
        "request to {path} failed{hint}: {}",
        err.without_url()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn status(code: u16) -> reqwest::StatusCode {
        reqwest::StatusCode::from_u16(code).unwrap()
    }

    fn client(base_url: &str) -> ApiClient {
        ApiClient::with_token(base_url, "test-token".to_string(), 5)
    }

    #[test]
    fn success_envelope_returns_data() {
        let body =
            r#"{"status":"success","data":{"id":7,"name":"pg"},"message":null,"status_code":200}"#;
        let data = classify(status(200), body).unwrap();
        assert_eq!(data["id"], 7);
        assert_eq!(data["name"], "pg");
    }

    #[test]
    fn envelope_error_status_is_an_error_even_on_200() {
        let body = r#"{"status":"error","data":null,"message":"boom","status_code":422}"#;
        let err = classify(status(200), body).unwrap_err();
        match err {
            Error::Other(err) => assert!(err.to_string().contains("boom")),
            other => panic!("expected Other, got {other:?}"),
        }
    }

    #[test]
    fn error_http_codes_map_to_typed_errors() {
        let body = r#"{"status":"error","message":"nope","status_code":401}"#;
        assert!(matches!(
            classify(status(401), body).unwrap_err(),
            Error::NotAuthenticated(_)
        ));

        let body = r#"{"status":"error","message":"pay up","status_code":402}"#;
        match classify(status(402), body).unwrap_err() {
            Error::BillingRequired(message) => assert_eq!(message, "pay up"),
            other => panic!("expected BillingRequired, got {other:?}"),
        }

        let body = r#"{"status":"error","message":"slow down","status_code":429}"#;
        match classify(status(429), body).unwrap_err() {
            Error::RateLimited(message) => assert_eq!(message, "slow down"),
            other => panic!("expected RateLimited, got {other:?}"),
        }
    }

    #[test]
    fn non_json_error_body_falls_back_to_status_and_snippet() {
        let err = classify(status(503), "<html>  upstream down </html>").unwrap_err();
        match err {
            Error::Other(err) => {
                let text = err.to_string();
                assert!(text.contains("HTTP 503"), "{text}");
                assert!(text.contains("upstream down"), "{text}");
            }
            other => panic!("expected Other, got {other:?}"),
        }
    }

    #[test]
    fn snippet_is_truncated() {
        let long = "x".repeat(ERROR_SNIPPET_CHARS + 20);
        let text = snippet(status(500), &long);
        assert!(text.starts_with("HTTP 500: "));
        assert!(text.ends_with('…'));
    }

    #[test]
    fn retry_after_parsing() {
        assert_eq!(parse_retry_after(Some("5")), 5);
        assert_eq!(parse_retry_after(Some(" 12 ")), 12);
        assert_eq!(parse_retry_after(Some("3600")), MAX_RETRY_AFTER_SECS);
        assert_eq!(parse_retry_after(Some("soon")), DEFAULT_RETRY_AFTER_SECS);
        assert_eq!(parse_retry_after(Some("-3")), DEFAULT_RETRY_AFTER_SECS);
        assert_eq!(parse_retry_after(None), DEFAULT_RETRY_AFTER_SECS);

        use reqwest::header::HeaderValue;
        let mut headers = HeaderMap::new();
        headers.insert(RETRY_AFTER, HeaderValue::from_static("7"));
        assert_eq!(retry_after_secs(&headers), 7);
        assert_eq!(
            retry_after_secs(&HeaderMap::new()),
            DEFAULT_RETRY_AFTER_SECS
        );
    }

    #[test]
    fn org_query_injection() {
        let plain = client("http://example.test");
        assert_eq!(plain.scoped_query(&[]), Vec::new());

        let scoped = client("http://example.test").with_org(Some("org_1".to_string()));
        assert_eq!(scoped.scoped_query(&[]), vec![("organization_id", "org_1")]);
        assert_eq!(
            scoped.scoped_query(&[("limit", "5")]),
            vec![("limit", "5"), ("organization_id", "org_1")]
        );
        // An explicit organization_id wins.
        assert_eq!(
            scoped.scoped_query(&[("organization_id", "explicit")]),
            vec![("organization_id", "explicit")]
        );
    }

    #[test]
    fn org_body_injection() {
        let scoped = client("http://example.test").with_org(Some("org_1".to_string()));

        let body = scoped.scoped_body(serde_json::json!({"name": "n"}));
        assert_eq!(body["organization_id"], "org_1");
        assert_eq!(body["name"], "n");

        let explicit = scoped.scoped_body(serde_json::json!({"organization_id": "other"}));
        assert_eq!(explicit["organization_id"], "other");

        let array = scoped.scoped_body(serde_json::json!([1, 2]));
        assert_eq!(array, serde_json::json!([1, 2]));
    }

    #[test]
    fn redaction_masks_secret_keys_only() {
        let mut value = serde_json::json!({
            "name": "web",
            "org": "acme",
            "password": "hunter2",
            "nested": {"refresh_token": "rt", "keep": 1},
            "list": [{"api_key": "k"}, {"secret_key": "s"}],
            "Secret": "case-insensitive",
        });
        redact_value(&mut value);

        assert_eq!(value["name"], "web");
        assert_eq!(value["org"], "acme");
        assert_eq!(value["password"], "[REDACTED]");
        assert_eq!(value["nested"]["refresh_token"], "[REDACTED]");
        assert_eq!(value["nested"]["keep"], 1);
        assert_eq!(value["list"][0]["api_key"], "[REDACTED]");
        assert_eq!(value["list"][1]["secret_key"], "[REDACTED]");
        assert_eq!(value["Secret"], "[REDACTED]");
    }

    #[test]
    fn redaction_masks_secret_substrings_case_insensitively() {
        let mut value = serde_json::json!({
            "id_token": "id",
            "accessToken": "at",
            "apiKey": "ak",
            "firebase_refresh_token": "frt",
            "private_key": "pk",
            "userCredential": "cred",
            "name": "web",
            "org": "acme",
            "console_url": "https://console.selfhost.dev",
        });
        redact_value(&mut value);

        for key in [
            "id_token",
            "accessToken",
            "apiKey",
            "firebase_refresh_token",
            "private_key",
            "userCredential",
        ] {
            assert_eq!(value[key], "[REDACTED]", "{key} must be masked");
        }
        assert_eq!(value["name"], "web");
        assert_eq!(value["org"], "acme");
        assert_eq!(value["console_url"], "https://console.selfhost.dev");
    }

    #[test]
    fn echo_shows_method_and_path_but_never_the_token() {
        let api = client("http://example.test").with_verbosity(true, true);
        let body = serde_json::json!({"name": "pg", "refresh_token": "rt"});
        let echo = api
            .echo_lines(&Method::POST, "/v1/postgres", Some(&body))
            .join("\n");

        assert!(echo.contains("→ POST /v1/postgres"), "{echo}");
        assert!(echo.contains("Authorization: Bearer [REDACTED]"), "{echo}");
        assert!(echo.contains(r#""refresh_token": "[REDACTED]""#), "{echo}");
        assert!(echo.contains(r#""name": "pg""#), "{echo}");
        assert!(!echo.contains("test-token"), "{echo}");
        assert!(!echo.contains(r#""rt""#), "{echo}");

        // Verbose alone is exactly the one summary line.
        let terse = client("http://example.test").with_verbosity(true, false);
        assert_eq!(
            terse.echo_lines(&Method::GET, "/v1/postgres", None),
            vec!["→ GET /v1/postgres".to_string()]
        );

        // Neither flag prints nothing.
        let silent = client("http://example.test");
        assert!(
            silent
                .echo_lines(&Method::GET, "/v1/postgres", None)
                .is_empty()
        );
    }

    fn http_response(status_line: &str, extra: &str, body: &str) -> String {
        format!(
            "HTTP/1.1 {status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{extra}Connection: close\r\n\r\n{body}",
            body.len()
        )
    }

    fn request_complete(buf: &[u8]) -> bool {
        let Some(end) = buf.windows(4).position(|window| window == b"\r\n\r\n") else {
            return false;
        };
        let head = String::from_utf8_lossy(&buf[..end]).to_ascii_lowercase();
        let length = head
            .lines()
            .find_map(|line| {
                line.strip_prefix("content-length:")
                    .map(|value| value.trim().parse::<usize>().unwrap_or(0))
            })
            .unwrap_or(0);
        buf.len() >= end + 4 + length
    }

    /// Serve the canned responses in order on a loopback port, recording every
    /// request line-and-body it received. A request is recorded before its
    /// response is written, so `try_iter` after an awaited call always sees it.
    async fn serve(responses: Vec<String>) -> (String, std::sync::mpsc::Receiver<Vec<u8>>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (sink, captured) = std::sync::mpsc::channel();
        tokio::spawn(async move {
            for response in responses {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut buf = Vec::new();
                let mut chunk = [0u8; 1024];
                loop {
                    let read = socket.read(&mut chunk).await.unwrap();
                    if read == 0 {
                        break;
                    }
                    buf.extend_from_slice(&chunk[..read]);
                    if request_complete(&buf) {
                        break;
                    }
                }
                sink.send(buf).unwrap();
                socket.write_all(response.as_bytes()).await.unwrap();
                socket.shutdown().await.ok();
            }
        });
        (format!("http://{addr}"), captured)
    }

    /// The recorded requests, in order.
    fn drain(captured: &std::sync::mpsc::Receiver<Vec<u8>>) -> Vec<String> {
        captured
            .try_iter()
            .map(|bytes| String::from_utf8_lossy(&bytes).to_string())
            .collect()
    }

    #[tokio::test]
    async fn live_success_envelope_returns_data() {
        let body = r#"{"status":"success","data":{"id":1},"message":null,"status_code":200}"#;
        let (base, captured) = serve(vec![http_response("200 OK", "", body)]).await;

        let data = client(&base)
            .get("/v1/postgres", &[("limit", "5")])
            .await
            .unwrap();
        assert_eq!(data["id"], 1);

        let request = drain(&captured).join("");
        assert!(request.starts_with("GET /v1/postgres?limit=5"), "{request}");
        assert!(
            request
                .to_ascii_lowercase()
                .contains("authorization: bearer test-token"),
            "{request}"
        );
    }

    #[tokio::test]
    async fn live_rate_limit_is_retried_once_then_succeeds() {
        let limited = http_response("429 Too Many Requests", "Retry-After: 1\r\n", "");
        let body = r#"{"status":"success","data":{"ok":true},"message":null,"status_code":200}"#;
        let (base, captured) = serve(vec![limited, http_response("200 OK", "", body)]).await;

        let data = client(&base).get("/v1/postgres", &[]).await.unwrap();
        assert_eq!(data["ok"], true);

        let requests = drain(&captured).join("");
        assert_eq!(
            requests.matches("GET /v1/postgres").count(),
            2,
            "{requests}"
        );
    }

    #[tokio::test]
    async fn live_org_injection_scoped_but_not_unscoped() {
        let body = r#"{"status":"success","data":{},"message":null,"status_code":200}"#;
        let (base, captured) = serve(vec![
            http_response("200 OK", "", body),
            http_response("200 OK", "", body),
            http_response("200 OK", "", body),
        ])
        .await;

        let api = client(&base).with_org(Some("org_1".to_string()));
        api.get("/v1/postgres", &[]).await.unwrap();
        api.get_unscoped("/v1/me", &[]).await.unwrap();
        api.post("/v1/postgres", serde_json::json!({"name": "pg"}))
            .await
            .unwrap();

        let requests = drain(&captured).join("\n---\n");
        assert!(
            requests.contains("GET /v1/postgres?organization_id=org_1"),
            "{requests}"
        );
        assert!(!requests.contains("/v1/me?organization_id"), "{requests}");
        assert!(
            requests.contains(r#""organization_id":"org_1""#),
            "{requests}"
        );
    }

    /// The three write methods reach the wire as their own verb and carry the
    /// injected organization in the body, so the invitation routes that resolve
    /// the organization from the body work.
    #[tokio::test]
    async fn live_put_patch_and_delete_use_their_verbs_and_bodies() {
        let body = r#"{"status":"success","data":null,"message":null,"status_code":200}"#;
        let (base, captured) = serve(vec![
            http_response("200 OK", "", body),
            http_response("200 OK", "", body),
            http_response("200 OK", "", body),
        ])
        .await;

        let api = client(&base).with_org(Some("org_1".to_string()));
        api.put(
            "/organizations/org_1",
            serde_json::json!({"organization": {"name": "Acme"}}),
        )
        .await
        .unwrap();
        api.patch(
            "/organizations/org_1/members/user_1/role",
            serde_json::json!({"role_pid": "role_admin"}),
        )
        .await
        .unwrap();
        api.delete("/organizations/org_1", serde_json::json!({}))
            .await
            .unwrap();

        let requests = drain(&captured).join("\n---\n");
        assert!(
            requests.contains("PUT /organizations/org_1 HTTP/1.1"),
            "{requests}"
        );
        assert!(
            requests.contains("PATCH /organizations/org_1/members/user_1/role HTTP/1.1"),
            "{requests}"
        );
        assert!(
            requests.contains("DELETE /organizations/org_1 HTTP/1.1"),
            "{requests}"
        );
        // Every write body is scoped, including DELETE's empty one.
        assert_eq!(
            requests.matches(r#""organization_id":"org_1""#).count(),
            3,
            "{requests}"
        );
        assert!(
            requests.contains(r#""role_pid":"role_admin""#),
            "{requests}"
        );
    }
}
