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
        Value::String(text) => text.clone(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => number.to_string(),
        nested => compact(nested),
    }
}

fn compact(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| String::new())
}
