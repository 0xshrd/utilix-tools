//! Öffnen/Speichern-Dialoge (ComDlg32\OpenSavePidlMRU): zuletzt gewählte Datei je Dateiendung.
//! Windows speichert nur für den neuesten Eintrag einer Endung einen Zeitpunkt (die Schreibzeit des Schlüssels).

use super::Collection;
use crate::model::{Activity, Kind};
use crate::reg::{HKEY_CURRENT_USER, Key};
use crate::shellitem;
use crate::util::u32_at;

const ROOT: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\ComDlg32\OpenSavePidlMRU";

pub fn collect(out: &mut Collection) {
    let Some(root) = Key::open(HKEY_CURRENT_USER, ROOT) else {
        return;
    };
    for (extension, written) in root.subkeys() {
        // "*" fasst alle Endungen zusammen und würde Einträge doppeln.
        if extension == "*" {
            continue;
        }
        let (Some(time), Some(key)) = (written, root.open_sub(&extension)) else {
            continue;
        };
        let Some(newest) = key.binary("MRUListEx").and_then(|list| u32_at(&list, 0)) else {
            continue;
        };
        let Some(pidl) = key.binary(&newest.to_string()) else {
            continue;
        };
        let segments: Vec<_> = shellitem::split_pidl(&pidl)
            .into_iter()
            .map(shellitem::parse)
            .collect();
        let path = shellitem::join(&segments);
        if path.is_empty() {
            continue;
        }
        out.activities.push(Activity {
            time,
            kind: Kind::FileSelected,
            description: path,
            details: String::new(),
            source: format!(r"HKCU\{ROOT}\{extension}"),
            extension: extension.to_lowercase(),
        });
    }
}
