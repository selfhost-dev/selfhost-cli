//! Render command results as a table, JSON or YAML (design §6).
//!
//! `table` is the default on a TTY, `json` when piped; `--format yaml` is explicit.
//! Human-facing errors go to stderr (see [`crate::error::Error`]) so that piping
//! stdout stays machine-readable.
//!
//! Slice 0 implements the format layer; every command slice feeds it
//! `serde_json::Value` payloads.

#![allow(dead_code)] // Slice 0: `render` is the entry point later slices call.

use std::io::IsTerminal as _;

use clap::ValueEnum;
use comfy_table::{ContentArrangement, Table, presets::UTF8_FULL};
use serde_json::Value;

use crate::error::{Error, Result};

/// Output format, `-o/--format`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Format {
    /// Aligned columns (default on a TTY).
    Table,
    /// JSON, pretty-printed.
    Json,
    /// YAML.
    Yaml,
}

impl Format {
    /// Effective format: `--json` wins, then `--format`, then TTY sniffing.
    pub fn resolve(explicit: Option<Self>, json_flag: bool) -> Self {
        if json_flag {
            Self::Json
        } else if let Some(format) = explicit {
            format
        } else if std::io::stdout().is_terminal() {
            Self::Table
        } else {
            Self::Json
        }
    }

    /// Render a value in this format, ready to be written to stdout.
    pub fn render(&self, value: &Value) -> Result<String> {
        match self {
            Self::Json => {
                serde_json::to_string_pretty(value).map_err(|err| Error::Other(err.into()))
            }
            Self::Yaml => serde_yaml_ng::to_string(value).map_err(|err| Error::Other(err.into())),
            Self::Table => Ok(render_table(value)),
        }
    }
}

/// Tables read best as one row per record: an array of objects becomes columns, a
/// single object becomes a two-column `field | value` sheet, scalars stay as-is.
fn render_table(value: &Value) -> String {
    match value {
        Value::Array(items) if items.is_empty() => "(no rows)".to_string(),
        Value::Array(items) => {
            let mut table = new_table();
            let columns = collect_columns(items);
            table.set_header(columns.iter().map(|c| c.as_str()).collect::<Vec<_>>());
            for item in items {
                let row: Vec<String> = columns
                    .iter()
                    .map(|column| cell(item.get(column).unwrap_or(&Value::Null)))
                    .collect();
                table.add_row(row);
            }
            table.to_string()
        }
        Value::Object(object) => {
            let mut table = new_table();
            table.set_header(vec!["field", "value"]);
            for (key, value) in object {
                table.add_row(vec![key.clone(), cell(value)]);
            }
            table.to_string()
        }
        scalar => cell(scalar),
    }
}

fn new_table() -> Table {
    let mut table = Table::new();
    table
        .load_style(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic);
    table
}

/// Union of the object keys, in first-seen order, so ragged rows still line up.
fn collect_columns(items: &[Value]) -> Vec<String> {
    let mut columns: Vec<String> = Vec::new();
    for item in items {
        if let Value::Object(object) = item {
            for key in object.keys() {
                if !columns.iter().any(|known| known == key) {
                    columns.push(key.clone());
                }
            }
        }
    }
    if columns.is_empty() {
        columns.push("value".to_string());
    }
    columns
}

fn cell(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => strip_control_characters(text),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => number.to_string(),
        nested => compact(nested),
    }
}

/// Server data reaches the terminal through a table cell, and this CLI is the
/// first to render third-party strings (organization names and slugs, member
/// names and emails, activity actors). Strip control characters — Unicode `Cc`,
/// which covers ESC, C0/C1, DEL, CR and LF — so a hostile string can never drive
/// the terminal. Also strip the bidi overrides/isolates (`U+202A..=U+202E`,
/// `U+2066..=U+2069`): they are not `Cc`, but they reorder the text around them
/// and so spoof a table just as well. The directional marks `U+200E`/`U+200F`
/// are legitimate and stay. JSON and YAML output stay byte-exact and skip this.
pub fn strip_control_characters(text: &str) -> String {
    text.chars()
        .filter(|character| {
            !character.is_control()
                && !matches!(*character, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
        })
        .collect()
}

fn compact(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| String::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A server-supplied name carrying a terminal escape (OSC 52, followed by
    /// BEL) must render without the control bytes that would drive the terminal.
    #[test]
    fn table_cells_drop_terminal_control_sequences() {
        let value = json!([{
            "name": "Acme\u{1b}]52;c;cGF5bG9hZA==\u{7}",
            "role": "owner",
        }]);

        let table = render_table(&value);
        assert!(!table.contains('\u{1b}'), "{table:?}");
        assert!(!table.contains('\u{7}'), "{table:?}");
        assert!(table.contains("Acme]52;c;cGF5bG9hZA=="), "{table:?}");
    }

    /// A right-to-left override in a cell reorders the text around it, so a
    /// hostile name could spoof the tail of a row. The override goes the way of
    /// any control character; the plain text around it survives.
    #[test]
    fn table_cells_drop_direction_overrides() {
        let value = json!([{
            "name": "\u{202e}Acme",
            "role": "owner",
        }]);

        let table = render_table(&value);
        assert!(!table.contains('\u{202e}'), "{table:?}");
        assert!(table.contains("Acme"), "{table:?}");
    }

    /// The exact codepoints the widening covers, and the two directional marks
    /// it deliberately leaves alone: the boundary is neither wider nor narrower.
    #[test]
    fn table_cells_drop_every_bidi_override_but_keep_the_marks() {
        for (codepoint, dropped) in [
            ('\u{202a}', true),
            ('\u{202b}', true),
            ('\u{202c}', true),
            ('\u{202d}', true),
            ('\u{202e}', true),
            ('\u{2066}', true),
            ('\u{2067}', true),
            ('\u{2068}', true),
            ('\u{2069}', true),
            ('\u{200e}', false),
            ('\u{200f}', false),
        ] {
            let stripped = strip_control_characters(&format!("a{codepoint}b"));
            assert_eq!(stripped == "ab", dropped, "U+{:04X}: {stripped:?}", codepoint as u32);
        }
    }

    /// JSON and YAML are machine-readable and stay byte-exact: the escape that a
    /// table drops is still present, only escaped by the serializer.
    #[test]
    fn json_output_keeps_the_control_characters() {
        let value = json!({"name": "Acme\u{1b}]52;c;cGF5bG9hZA==\u{7}"});
        let rendered = Format::Json.render(&value).unwrap();
        assert!(rendered.contains("\\u001b"), "{rendered}");
        assert!(rendered.contains("\\u0007"), "{rendered}");
    }
}
