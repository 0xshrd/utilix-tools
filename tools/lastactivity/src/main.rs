#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod export;
mod model;
mod sources;
mod util;

use export::Format;
use std::path::PathBuf;

const USAGE: &str = "\
LastActivity – Zeitachse der PC-Nutzung (utilix)

Ohne Argumente startet die Oberfläche. Für den Export ohne Oberfläche:

  lastactivity.exe --csv <Datei> [--json <Datei>] [--html <Datei>]
";

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        return app::run();
    }
    std::process::exit(cli(&args));
}

fn cli(args: &[String]) -> i32 {
    attach_parent_console();

    let mut targets: Vec<(Format, PathBuf)> = Vec::new();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        let format = match arg.as_str() {
            "--csv" => Format::Csv,
            "--json" => Format::Json,
            "--html" => Format::Html,
            "-h" | "--help" | "/?" => {
                print!("{USAGE}");
                return 0;
            }
            other => {
                eprintln!("Unbekanntes Argument: {other}\n\n{USAGE}");
                return 2;
            }
        };
        let Some(path) = iter.next() else {
            eprintln!("Nach {arg} fehlt der Dateiname.");
            return 2;
        };
        targets.push((format, PathBuf::from(path)));
    }

    let data = sources::collect_all();
    for warning in &data.warnings {
        eprintln!("Hinweis: {warning}");
    }
    let rows: Vec<_> = data.activities.iter().collect();
    for (format, path) in targets {
        if let Err(e) = export::write(&path, format, &rows) {
            eprintln!("{}: {e}", path.display());
            return 1;
        }
        println!("{} Einträge gespeichert: {}", rows.len(), path.display());
    }
    0
}

/// Im Release ist das Programm eine GUI-Anwendung; für Ausgaben hängen wir uns an die aufrufende Konsole.
fn attach_parent_console() {
    use windows::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
    unsafe {
        let _ = AttachConsole(ATTACH_PARENT_PROCESS);
    }
}
