//! `api` — call any platform endpoint directly.
//!
//! The escape hatch for endpoints with no typed command yet: the endpoint, the
//! method, parameters, headers and output behave like the rest of the CLI
//! (profile sign-in, organization handling, envelope errors, exit codes).

use std::str::FromStr as _;

use clap::Args;
use reqwest::Method;
use serde_json::Value;

use crate::api::{RawBody, RawData, classify_raw};
use crate::config::ProfileStore;
use crate::error::{Error, Result};
use crate::output::strip_control_characters;

use super::{
    GlobalArgs, human_output, no_organization_selected, org_reference, print, reject_dry_run_for,
    resolve_org_pid, unscoped_client,
};

/// Headers the CLI manages itself: passed explicitly they fail the command
/// instead of being overridden or silently dropped.
const BLOCKED_HEADERS: [&str; 4] = ["authorization", "accept", "content-type", "content-length"];

// Arguments for `selfhost api`.
#[derive(Debug, Clone, Args)]
pub struct ApiArgs {
    /// Endpoint path, with or without a leading slash
    #[arg(value_name = "ENDPOINT")]
    pub endpoint: String,

    /// How to call it: GET, POST, PUT, PATCH or DELETE; pass GET to keep parameters on the query string
    #[arg(short = 'X', long = "method", value_name = "METHOD")]
    pub method: Option<String>,

    /// Plain text parameter as KEY=VALUE (repeatable); adding one turns the request into a POST unless you name another method
    #[arg(short = 'f', long = "raw-field", value_name = "KEY=VALUE")]
    pub raw_fields: Vec<String>,

    /// Parameter as KEY=VALUE, typed when possible; a value starting with @ reads a file or stdin (repeatable); adding one turns the request into a POST unless you name another method
    #[arg(short = 'F', long = "field", value_name = "KEY=VALUE")]
    pub fields: Vec<String>,

    /// Send a file, or stdin with a dash, as the request body
    #[arg(long = "input", value_name = "FILE")]
    pub input: Option<String>,

    /// Extra header as NAME: VALUE (repeatable)
    #[arg(short = 'H', long = "header", value_name = "NAME: VALUE")]
    pub headers: Vec<String>,

    /// Show the status line and headers with the response
    #[arg(short = 'i', long = "include")]
    pub include: bool,

    /// Print nothing; the exit code says how it went
    #[arg(long = "silent")]
    pub silent: bool,
}

/// `selfhost api`: validate everything, resolve the organization, send one raw
/// request and print the envelope data (or the raw text for non-JSON bodies).
pub async fn run(global: &GlobalArgs, args: ApiArgs) -> Result<()> {
    reject_dry_run_for(global, "api calls")?;
    let sends_fields = !args.raw_fields.is_empty() || !args.fields.is_empty();
    let method = resolve_method(args.method.as_deref(), sends_fields || args.input.is_some())?;
    let endpoint = normalize_endpoint(&args.endpoint)?;
    let merged = merge_fields(
        parse_raw_fields(&args.raw_fields)?,
        parse_fields(&args.fields).await?,
    );
    let input = match args.input.as_deref() {
        Some("-") => Some(read_stdin_bytes().await?),
        Some(path) => Some(read_input_file(path)?),
        None => None,
    };
    let headers = parse_headers(&args.headers)?;

    let mut store = ProfileStore::load()?;
    let name = store.resolved_name(global.profile.as_deref())?;
    let reference = {
        let profile = store.require_profile(&name)?;
        org_reference(None, global, profile).map(str::to_owned)
    };
    if endpoint.contains("{org}") && reference.is_none() {
        return Err(no_organization_selected());
    }
    let unscoped = unscoped_client(&mut store, &name, global).await?;
    let (client, pid) = match reference {
        Some(reference) => {
            let pid = resolve_org_pid(&unscoped, &reference).await?;
            (unscoped.with_org(Some(pid.clone())), Some(pid))
        }
        None => (unscoped, None),
    };
    let path = substitute_org(&endpoint, pid.as_deref())?;

    let (query, planned) = plan(&method, merged, input);
    let body = match &planned {
        PlannedBody::Json(value) => Some(RawBody::Json(value.clone())),
        PlannedBody::Raw(bytes) => Some(RawBody::Raw(bytes.clone())),
        PlannedBody::None => None,
    };
    let response = client
        .raw(method.clone(), &path, &query, body, &headers)
        .await?;

    let data = classify_raw(response.status, &response.body)?;
    if args.silent {
        return Ok(());
    }
    if args.include {
        let rendered = render_body(global, &data)?;
        println!(
            "{}",
            format_include(response.status, &response.headers, &rendered)
        );
    } else {
        match &data {
            RawData::Json(value) => print(global, value)?,
            RawData::Text(text) => println!("{}", render_text(global, text)),
        }
    }
    Ok(())
}

/// The method for this run: an explicit `-X` wins (case-insensitively),
/// otherwise parameters or an input body upgrade the default GET to POST.
fn resolve_method(explicit: Option<&str>, sends_body: bool) -> Result<Method> {
    match explicit {
        Some(raw) => match raw.to_ascii_uppercase().as_str() {
            "GET" => Ok(Method::GET),
            "POST" => Ok(Method::POST),
            "PUT" => Ok(Method::PUT),
            "PATCH" => Ok(Method::PATCH),
            "DELETE" => Ok(Method::DELETE),
            _ => Err(Error::Usage(format!(
                "unknown method '{raw}'; pass one of GET, POST, PUT, PATCH, DELETE"
            ))),
        },
        None if sends_body => Ok(Method::POST),
        None => Ok(Method::GET),
    }
}

/// The endpoint as the request path: a leading slash is added when missing.
/// Full URLs, whitespace/control characters and `..` path segments are usage
/// errors; any `?...` already on the endpoint is preserved.
fn normalize_endpoint(raw: &str) -> Result<String> {
    if raw.is_empty() {
        return Err(Error::Usage(
            "endpoint is empty; pass a path such as /organizations".to_string(),
        ));
    }
    if raw.contains("://") {
        return Err(Error::Usage(
            "endpoint must be a path, not a full URL; pass a path such as /organizations"
                .to_string(),
        ));
    }
    if raw.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(Error::Usage(
            "endpoint must not contain whitespace or control characters".to_string(),
        ));
    }
    let (path, query) = match raw.split_once('?') {
        Some((path, query)) => (path, Some(query)),
        None => (raw, None),
    };
    if path.split('/').any(|segment| segment == "..") {
        return Err(Error::Usage(
            "endpoint must not contain .. path segments".to_string(),
        ));
    }
    let normalized = if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    };
    match query {
        Some(query) => Ok(format!("{normalized}?{query}")),
        None => Ok(normalized),
    }
}

/// Split one `KEY=VALUE` item; anything else (including an empty key) is a
/// usage error naming the shape.
fn split_key_value<'a>(item: &'a str, kind: &str) -> Result<(&'a str, &'a str)> {
    item.split_once('=')
        .filter(|(key, _)| !key.is_empty())
        .ok_or_else(|| Error::Usage(format!("{kind} '{item}' needs KEY=VALUE form")))
}

/// `-f` parameters: literal strings, no `@` expansion.
fn parse_raw_fields(raw: &[String]) -> Result<Vec<(String, Value)>> {
    raw.iter()
        .map(|item| {
            let (key, value) = split_key_value(item, "raw field")?;
            Ok((key.to_owned(), Value::String(value.to_owned())))
        })
        .collect()
}

/// `-F` parameters: `@file`/`@-` reads a file or stdin first, otherwise the
/// value is typed (`true`/`false`/`null`/integers), and everything else stays
/// a string.
async fn parse_fields(raw: &[String]) -> Result<Vec<(String, Value)>> {
    let mut out = Vec::with_capacity(raw.len());
    for item in raw {
        let (key, value) = split_key_value(item, "field")?;
        let value = match value.strip_prefix('@') {
            Some("-") => Value::String(read_stdin_string().await?),
            Some(path) => Value::String(read_field_file(path)?),
            None => typed_value(value),
        };
        out.push((key.to_owned(), value));
    }
    Ok(out)
}

/// The typed reading of one `-F` value.
fn typed_value(value: &str) -> Value {
    match value {
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        "null" => Value::Null,
        _ => value
            .parse::<i64>()
            .map(Value::from)
            .unwrap_or_else(|_| Value::String(value.to_owned())),
    }
}

/// Both field lists in order; the last value wins on duplicate keys.
fn merge_fields(raw: Vec<(String, Value)>, typed: Vec<(String, Value)>) -> Vec<(String, Value)> {
    let mut out: Vec<(String, Value)> = Vec::with_capacity(raw.len() + typed.len());
    for (key, value) in raw.into_iter().chain(typed) {
        match out.iter_mut().find(|(known, _)| *known == key) {
            Some((_, slot)) => *slot = value,
            None => out.push((key, value)),
        }
    }
    out
}

/// The planned body: none, a JSON object, or raw input bytes.
#[derive(Debug)]
enum PlannedBody {
    None,
    Json(Value),
    Raw(Vec<u8>),
}

/// Where the fields go: GET turns them into query pairs, writes into a JSON
/// object body, and with `--input` the fields go to the query while the input
/// bytes are the body.
fn plan(
    method: &Method,
    fields: Vec<(String, Value)>,
    input: Option<Vec<u8>>,
) -> (Vec<(String, String)>, PlannedBody) {
    if let Some(bytes) = input {
        let query = fields
            .into_iter()
            .map(|(key, value)| (key, query_value(&value)))
            .collect();
        return (query, PlannedBody::Raw(bytes));
    }
    if *method == Method::GET {
        let query = fields
            .into_iter()
            .map(|(key, value)| (key, query_value(&value)))
            .collect();
        return (query, PlannedBody::None);
    }
    let body = Value::Object(fields.into_iter().collect());
    (Vec::new(), PlannedBody::Json(body))
}

/// One field value as a query pair string.
fn query_value(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Null => "null".to_string(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => number.to_string(),
        nested => serde_json::to_string(nested).unwrap_or_default(),
    }
}

/// Repeatable `Name: value` headers, split on the first colon and trimmed.
/// Missing colons, empty names, unparseable pairs and the headers the CLI
/// manages itself are usage errors — the whole command fails rather than
/// sending with a header silently dropped.
fn parse_headers(raw: &[String]) -> Result<Vec<(String, String)>> {
    raw.iter()
        .map(|item| {
            let (name, value) = item
                .split_once(':')
                .ok_or_else(|| Error::Usage(format!("header '{item}' needs NAME: VALUE form")))?;
            let name = name.trim();
            let value = value.trim();
            if name.is_empty() {
                return Err(Error::Usage(format!(
                    "header '{item}' needs NAME: VALUE form"
                )));
            }
            if BLOCKED_HEADERS.contains(&name.to_ascii_lowercase().as_str()) {
                return Err(Error::Usage(format!(
                    "header '{name}' is managed by the CLI and cannot be overridden"
                )));
            }
            if reqwest::header::HeaderName::from_str(name).is_err() {
                return Err(Error::Usage(format!(
                    "header '{name}' is not a valid header name"
                )));
            }
            if reqwest::header::HeaderValue::from_str(value).is_err() {
                return Err(Error::Usage(format!(
                    "header '{name}' has an invalid value"
                )));
            }
            Ok((name.to_owned(), value.to_owned()))
        })
        .collect()
}

/// Fill `{org}` in the path with the resolved pid. Without a placeholder the
/// path passes through; with one but no organization the run stops with the
/// hint before any credential is read.
fn substitute_org(path: &str, pid: Option<&str>) -> Result<String> {
    if !path.contains("{org}") {
        return Ok(path.to_string());
    }
    let pid = pid.ok_or_else(no_organization_selected)?;
    Ok(path.replace("{org}", pid))
}

/// `--include` output: the status line, the response headers sorted by
/// lowercase name, a blank line, then the rendered body. Header values are
/// server-controlled, so control and bidi characters are stripped before they
/// reach the terminal.
fn format_include(status: reqwest::StatusCode, headers: &[(String, String)], body: &str) -> String {
    let mut sorted: Vec<(&str, &str)> = headers
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect();
    sorted.sort_by_key(|header| header.0.to_ascii_lowercase());
    let mut out = format!("HTTP {}", status.as_u16());
    for (name, value) in sorted {
        let value = strip_control_characters(value);
        out.push_str(&format!("\n{name}: {value}"));
    }
    out.push_str("\n\n");
    out.push_str(body);
    out
}

/// A non-JSON response body for the resolved format: table cells strip control
/// characters, JSON and YAML stay byte-exact.
fn render_text(global: &GlobalArgs, text: &str) -> String {
    if human_output(global) {
        strip_control_characters(text)
    } else {
        text.to_string()
    }
}

/// The response body for `--include`: envelope data rendered in the resolved
/// format, raw text otherwise.
fn render_body(global: &GlobalArgs, data: &RawData) -> Result<String> {
    match data {
        RawData::Json(value) => {
            let format = crate::output::Format::resolve(global.format, global.json);
            Ok(format.render(value)?)
        }
        RawData::Text(text) => Ok(render_text(global, text)),
    }
}

/// One `-F @file` value.
fn read_field_file(path: &str) -> Result<String> {
    std::fs::read_to_string(path)
        .map_err(|err| Error::Other(anyhow::anyhow!("cannot read file '{path}': {err}")))
}

/// One `--input` file, sent as-is.
fn read_input_file(path: &str) -> Result<Vec<u8>> {
    std::fs::read(path)
        .map_err(|err| Error::Other(anyhow::anyhow!("cannot read input file '{path}': {err}")))
}

/// Piped stdin as text (for `-F @-`).
async fn read_stdin_string() -> Result<String> {
    use tokio::io::AsyncReadExt as _;
    let mut text = String::new();
    tokio::io::stdin()
        .read_to_string(&mut text)
        .await
        .map_err(|err| Error::Other(anyhow::anyhow!("cannot read stdin: {err}")))?;
    Ok(text)
}

/// Piped stdin as bytes (for `--input -`), sent as-is.
async fn read_stdin_bytes() -> Result<Vec<u8>> {
    use tokio::io::AsyncReadExt as _;
    let mut bytes = Vec::new();
    tokio::io::stdin()
        .read_to_end(&mut bytes)
        .await
        .map_err(|err| Error::Other(anyhow::anyhow!("cannot read stdin: {err}")))?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn global() -> GlobalArgs {
        GlobalArgs {
            profile: None,
            base_url: None,
            org: None,
            format: None,
            json: false,
            no_color: false,
            timeout: 5,
            poll_interval: None,
            yes: false,
            dry_run: false,
            quiet: false,
            verbose: false,
            debug: false,
            help: None,
            version: None,
        }
    }

    fn status_code(code: u16) -> reqwest::StatusCode {
        reqwest::StatusCode::from_u16(code).unwrap()
    }

    #[test]
    fn an_explicit_method_wins_case_insensitively() {
        assert_eq!(resolve_method(Some("get"), false).unwrap(), Method::GET);
        assert_eq!(resolve_method(Some("Post"), true).unwrap(), Method::POST);
        assert_eq!(resolve_method(Some("PUT"), false).unwrap(), Method::PUT);
        assert_eq!(resolve_method(Some("patch"), false).unwrap(), Method::PATCH);
        assert_eq!(
            resolve_method(Some("DELETE"), false).unwrap(),
            Method::DELETE
        );
    }

    #[test]
    fn an_unknown_method_is_a_usage_error() {
        let err = resolve_method(Some("FROB"), false).unwrap_err();
        match err {
            Error::Usage(message) => {
                assert!(message.contains("FROB"), "{message}");
                assert!(message.contains("GET"), "{message}");
            }
            other => panic!("expected Usage, got {other:?}"),
        }
    }

    #[test]
    fn the_default_is_get_until_fields_or_input_upgrade_it_to_post() {
        assert_eq!(resolve_method(None, false).unwrap(), Method::GET);
        assert_eq!(resolve_method(None, true).unwrap(), Method::POST);
        // An explicit GET survives parameters.
        assert_eq!(resolve_method(Some("GET"), true).unwrap(), Method::GET);
    }

    #[test]
    fn endpoints_gain_a_leading_slash_and_keep_their_query() {
        assert_eq!(
            normalize_endpoint("organizations").unwrap(),
            "/organizations"
        );
        assert_eq!(
            normalize_endpoint("/organizations").unwrap(),
            "/organizations"
        );
        assert_eq!(
            normalize_endpoint("organizations?page=2").unwrap(),
            "/organizations?page=2"
        );
        assert_eq!(
            normalize_endpoint("/organizations?page=2").unwrap(),
            "/organizations?page=2"
        );
    }

    #[test]
    fn a_full_url_is_rejected() {
        for endpoint in [
            "https://api.selfhost.dev/organizations",
            "http://localhost:3000/organizations",
        ] {
            match normalize_endpoint(endpoint).unwrap_err() {
                Error::Usage(message) => assert!(message.contains("not a full URL"), "{message}"),
                other => panic!("expected Usage, got {other:?}"),
            }
        }
    }

    #[test]
    fn an_empty_endpoint_or_one_with_whitespace_is_rejected() {
        assert!(matches!(
            normalize_endpoint("").unwrap_err(),
            Error::Usage(_)
        ));
        for endpoint in ["/org anizations", "/organizations\t", "/org\u{7f}s"] {
            match normalize_endpoint(endpoint).unwrap_err() {
                Error::Usage(message) => assert!(message.contains("whitespace"), "{message}"),
                other => panic!("expected Usage, got {other:?}"),
            }
        }
    }

    #[test]
    fn traversal_segments_are_rejected_but_query_dots_pass() {
        for endpoint in ["/a/../b", "..", "/..", "a/../b"] {
            assert!(
                normalize_endpoint(endpoint).is_err(),
                "{endpoint} must be rejected"
            );
        }
        assert_eq!(normalize_endpoint("/search?q=..").unwrap(), "/search?q=..");
    }

    #[test]
    fn raw_fields_stay_literal_and_need_their_equals() {
        let parsed = parse_raw_fields(&["a=1".to_string(), "at=@-".to_string()]).unwrap();
        assert_eq!(parsed[0], ("a".to_string(), Value::String("1".to_string())));
        // No `@` expansion for raw fields.
        assert_eq!(parsed[1].1, Value::String("@-".to_string()));

        for item in ["no-equals", "=value"] {
            assert!(
                parse_raw_fields(&[item.to_string()]).is_err(),
                "{item} must be rejected"
            );
        }
    }

    #[tokio::test]
    async fn typed_fields_cover_booleans_null_and_integers() {
        let parsed = parse_fields(&[
            "t=true".to_string(),
            "f=false".to_string(),
            "n=null".to_string(),
            "i=42".to_string(),
            "neg=-7".to_string(),
            "s=hello".to_string(),
            "float=1.5".to_string(),
            "upper=True".to_string(),
        ])
        .await
        .unwrap();
        let get = |key: &str| {
            parsed
                .iter()
                .find(|(known, _)| known == key)
                .map(|(_, value)| value.clone())
        };
        assert_eq!(get("t"), Some(Value::Bool(true)));
        assert_eq!(get("f"), Some(Value::Bool(false)));
        assert_eq!(get("n"), Some(Value::Null));
        assert_eq!(get("i"), Some(serde_json::json!(42)));
        assert_eq!(get("neg"), Some(serde_json::json!(-7)));
        // Only integers are numeric; the rest stay strings.
        assert_eq!(get("s"), Some(Value::String("hello".to_string())));
        assert_eq!(get("float"), Some(Value::String("1.5".to_string())));
        assert_eq!(get("upper"), Some(Value::String("True".to_string())));

        assert!(parse_fields(&["bare".to_string()]).await.is_err());
    }

    #[tokio::test]
    async fn field_file_contents_arrive_untyped_whatever_the_bytes() {
        let path = std::env::temp_dir().join(format!("selfhost-api-field-{}", std::process::id()));
        std::fs::write(&path, "file text\n").expect("the fixture is written");
        let spec = format!("body=@{}", path.display());
        let parsed = parse_fields(&[spec]).await.unwrap();
        assert_eq!(parsed[0].1, Value::String("file text\n".to_string()));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn the_last_value_wins_on_duplicate_keys() {
        let merged = merge_fields(
            vec![("a".to_string(), Value::String("1".to_string()))],
            vec![
                ("a".to_string(), Value::String("2".to_string())),
                ("b".to_string(), Value::Bool(true)),
            ],
        );
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].1, Value::String("2".to_string()));
        assert_eq!(merged[1].1, Value::Bool(true));
    }

    #[test]
    fn get_requests_carry_the_fields_as_query_pairs() {
        let fields = vec![
            ("a".to_string(), Value::String("1".to_string())),
            ("t".to_string(), Value::Bool(true)),
            ("n".to_string(), Value::Null),
            ("i".to_string(), serde_json::json!(42)),
        ];
        let (query, body) = plan(&Method::GET, fields, None);
        assert_eq!(
            query,
            vec![
                ("a".to_string(), "1".to_string()),
                ("t".to_string(), "true".to_string()),
                ("n".to_string(), "null".to_string()),
                ("i".to_string(), "42".to_string()),
            ]
        );
        assert!(matches!(body, PlannedBody::None));
    }

    #[test]
    fn writes_carry_the_fields_as_a_json_object() {
        let fields = vec![("a".to_string(), Value::String("1".to_string()))];
        for method in [Method::POST, Method::PUT, Method::PATCH, Method::DELETE] {
            let (query, body) = plan(&method, fields.clone(), None);
            assert!(query.is_empty(), "{method}");
            match body {
                PlannedBody::Json(Value::Object(map)) => {
                    assert_eq!(map["a"], Value::String("1".to_string()));
                }
                other => panic!("expected a JSON object, got {other:?}"),
            }
        }
    }

    #[test]
    fn input_bytes_win_the_body_and_push_fields_to_the_query() {
        let fields = vec![("a".to_string(), Value::String("1".to_string()))];
        let (query, body) = plan(&Method::POST, fields, Some(b"[1,2]".to_vec()));
        assert_eq!(query, vec![("a".to_string(), "1".to_string())]);
        match body {
            PlannedBody::Raw(bytes) => assert_eq!(bytes, b"[1,2]"),
            other => panic!("expected raw bytes, got {other:?}"),
        }
    }

    #[test]
    fn headers_split_on_the_first_colon_and_trim() {
        let parsed = parse_headers(&["X-Token:  abc:def  ".to_string()]).unwrap();
        assert_eq!(parsed, vec![("X-Token".to_string(), "abc:def".to_string())]);
    }

    #[test]
    fn malformed_headers_are_usage_errors() {
        for item in ["no-colon", ": value", "   "] {
            assert!(
                parse_headers(&[item.to_string()]).is_err(),
                "{item} must be rejected"
            );
        }
    }

    #[test]
    fn managed_headers_fail_the_whole_command_whatever_the_case() {
        for name in [
            "authorization",
            "Authorization",
            "ACCEPT",
            "content-type",
            "Content-Length",
        ] {
            match parse_headers(&[format!("{name}: x")]).unwrap_err() {
                Error::Usage(message) => assert!(message.contains(name), "{message}"),
                other => panic!("expected Usage, got {other:?}"),
            }
        }
        // Allowed headers pass untouched.
        assert_eq!(
            parse_headers(&["X-Trace: 1".to_string()]).unwrap(),
            vec![("X-Trace".to_string(), "1".to_string())]
        );
    }

    #[test]
    fn the_org_placeholder_needs_a_resolved_organization() {
        assert_eq!(
            substitute_org("/organizations", Some("org_1")).unwrap(),
            "/organizations"
        );
        assert_eq!(
            substitute_org("/organizations/{org}/members", Some("org_1")).unwrap(),
            "/organizations/org_1/members"
        );
        match substitute_org("/organizations/{org}/members", None).unwrap_err() {
            Error::Usage(message) => assert!(message.contains("org use"), "{message}"),
            other => panic!("expected Usage, got {other:?}"),
        }
    }

    #[test]
    fn include_output_sorts_headers_and_separates_the_body() {
        let out = format_include(
            status_code(200),
            &[
                ("X-Trace".to_string(), "1".to_string()),
                ("content-type".to_string(), "application/json".to_string()),
            ],
            "{}",
        );
        assert_eq!(
            out,
            "HTTP 200\ncontent-type: application/json\nX-Trace: 1\n\n{}"
        );
    }

    #[test]
    fn include_output_drops_control_sequences_from_header_values() {
        let out = format_include(
            status_code(200),
            &[("X-Trace".to_string(), "\u{1b}]52;c;xyz\u{7}".to_string())],
            "{}",
        );
        let value_line = out
            .lines()
            .find(|line| line.starts_with("X-Trace:"))
            .unwrap();
        assert!(!value_line.chars().any(|c| c.is_control()), "{out:?}");
        assert!(out.contains("X-Trace: ]52;c;xyz"), "{out:?}");
    }

    #[test]
    fn table_text_strips_control_characters_while_json_stays_exact() {
        let dirty = "a\x1bb\x07c";
        let mut table = global();
        table.format = Some(crate::output::Format::Table);
        assert_eq!(render_text(&table, dirty), "abc");
        let mut piped = global();
        piped.json = true;
        assert_eq!(render_text(&piped, dirty), dirty);
    }
}
