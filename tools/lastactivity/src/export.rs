//! Export nach CSV, JSON und HTML. Gleiches Format in allen utilix-Tools.

use crate::model::Activity;
use chrono::Local;
use std::fs;
use std::io;
use std::path::Path;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Format {
    Csv,
    Json,
    Html,
}

impl Format {
    pub const ALL: [Format; 3] = [Format::Csv, Format::Json, Format::Html];

    pub fn extension(self) -> &'static str {
        match self {
            Format::Csv => "csv",
            Format::Json => "json",
            Format::Html => "html",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Format::Csv => "CSV (Excel)",
            Format::Json => "JSON",
            Format::Html => "HTML-Bericht",
        }
    }
}

const HEADERS: [&str; 6] = [
    "Zeit",
    "Aktion",
    "Beschreibung",
    "Details",
    "Quelle",
    "Dateiendung",
];

fn fields(a: &Activity) -> [String; 6] {
    [
        a.local_time(),
        a.kind.label().to_string(),
        a.description.clone(),
        a.details.clone(),
        a.source.clone(),
        a.extension.clone(),
    ]
}

pub fn write(path: &Path, format: Format, rows: &[&Activity]) -> io::Result<()> {
    let content = match format {
        Format::Csv => csv(rows),
        Format::Json => json(rows),
        Format::Html => html(rows),
    };
    fs::write(path, content)
}

/// Semikolon und BOM, damit ein deutsches Excel Spalten und Umlaute direkt richtig erkennt.
fn csv(rows: &[&Activity]) -> String {
    let quote = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
    let mut out = String::from("\u{feff}");
    out.push_str(&HEADERS.map(quote).join(";"));
    out.push_str("\r\n");
    for a in rows {
        out.push_str(&fields(a).map(|f| quote(&f)).join(";"));
        out.push_str("\r\n");
    }
    out
}

fn json(rows: &[&Activity]) -> String {
    let items: Vec<_> = rows
        .iter()
        .map(|a| {
            serde_json::json!({
                "time": a.time.with_timezone(&Local).to_rfc3339(),
                "action": a.kind.label(),
                "description": a.description,
                "details": a.details,
                "source": a.source,
                "extension": a.extension,
            })
        })
        .collect();
    serde_json::to_string_pretty(&items).unwrap_or_default()
}

fn html(rows: &[&Activity]) -> String {
    let escape = |s: &str| {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
    };
    let mut out = String::from(
        "<!doctype html>\n<html lang=\"de\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
         <title>LastActivity-Bericht</title><style>\
         body{font:14px system-ui,sans-serif;margin:24px;background:#0b140b;color:#e8efe4}\
         h1{font-size:20px;color:#bfcf5b}p{color:#9fb09a}\
         table{border-collapse:collapse;width:100%}\
         th,td{padding:6px 10px;border-bottom:1px solid #22351f;text-align:left;vertical-align:top}\
         th{position:sticky;top:0;background:#0e330d;color:#ddf8d7}\
         tr:nth-child(even) td{background:#101c10}td:first-child{white-space:nowrap}\
         </style></head><body>\n",
    );
    out.push_str(&format!(
        "<h1>LastActivity-Bericht</h1><p>{} Einträge · erstellt am {} · utilix</p>\n<table><thead><tr>",
        rows.len(),
        Local::now().format("%d.%m.%Y %H:%M")
    ));
    for h in HEADERS {
        out.push_str(&format!("<th>{h}</th>"));
    }
    out.push_str("</tr></thead><tbody>\n");
    for a in rows {
        out.push_str("<tr>");
        for f in fields(a) {
            out.push_str(&format!("<td>{}</td>", escape(&f)));
        }
        out.push_str("</tr>\n");
    }
    out.push_str("</tbody></table></body></html>\n");
    out
}
