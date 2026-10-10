//! Zuletzt verwendet (%APPDATA%\Microsoft\Windows\Recent): Verknüpfungen auf geöffnete Dateien und Ordner.

use super::Collection;
use crate::model::{Activity, Kind};
use crate::util::{extension_of, latin1_z, u16_at, u32_at, utf16_z};
use chrono::{DateTime, Utc};
use std::fs;
use std::path::PathBuf;

pub fn collect(out: &mut Collection) {
    let Ok(appdata) = std::env::var("APPDATA") else {
        return;
    };
    let dir = PathBuf::from(appdata).join(r"Microsoft\Windows\Recent");
    let Ok(entries) = fs::read_dir(&dir) else {
        out.warnings
            .push(format!("Zuletzt verwendet: {} nicht lesbar", dir.display()));
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("lnk"))
        {
            continue;
        }
        // Windows aktualisiert die Verknüpfung bei jedem Öffnen; ihre Änderungszeit ist der Zeitpunkt.
        let Some(time) = entry.metadata().ok().and_then(|m| m.modified().ok()) else {
            continue;
        };
        let target = fs::read(&path).ok().and_then(|b| link_target(&b));
        let (description, is_dir) = match target {
            Some((target, is_dir)) => (target, is_dir),
            None => (
                path.file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                false,
            ),
        };
        out.activities.push(Activity {
            time: DateTime::<Utc>::from(time),
            kind: if is_dir {
                Kind::FolderOpened
            } else {
                Kind::FileOpened
            },
            extension: if is_dir {
                String::new()
            } else {
                extension_of(&description)
            },
            description,
            details: String::new(),
            source: path.display().to_string(),
        });
    }
}

/// Zielpfad und "ist Ordner" aus einer .lnk-Datei (MS-SHLLINK) lesen.
fn link_target(b: &[u8]) -> Option<(String, bool)> {
    if u32_at(b, 0)? != 0x4C {
        return None;
    }
    let flags = u32_at(b, 0x14)?;
    let is_dir = u32_at(b, 0x18)? & 0x10 != 0;
    let mut off = 0x4C;
    if flags & 0x1 != 0 {
        // LinkTargetIDList überspringen
        off += 2 + u16_at(b, off)? as usize;
    }
    if flags & 0x2 == 0 {
        return None;
    }

    let info = off;
    let header_size = u32_at(b, info + 4)?;
    let info_flags = u32_at(b, info + 8)?;
    let unicode = header_size >= 0x24;
    let string_at = |field: usize| -> Option<String> {
        let rel = u32_at(b, info + field)? as usize;
        if rel == 0 {
            return None;
        }
        if unicode && (field == 0x1C || field == 0x20) {
            utf16_z(b, info + rel)
        } else {
            latin1_z(b, info + rel)
        }
    };

    let base = if info_flags & 0x1 != 0 {
        if unicode {
            string_at(0x1C)
        } else {
            string_at(0x10)
        }?
    } else if info_flags & 0x2 != 0 {
        // Netzwerkpfad: CommonNetworkRelativeLink → NetName
        let net = info + u32_at(b, info + 0x14)? as usize;
        latin1_z(b, net + u32_at(b, net + 8)? as usize)?
    } else {
        return None;
    };
    let suffix = if unicode {
        string_at(0x20)
    } else {
        string_at(0x18)
    }
    .unwrap_or_default();

    let target = if suffix.is_empty() {
        base
    } else if base.ends_with('\\') {
        format!("{base}{suffix}")
    } else {
        format!("{base}\\{suffix}")
    };
    Some((target, is_dir))
}
