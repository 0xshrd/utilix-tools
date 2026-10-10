//! Installierte Software aus den Uninstall-Schlüsseln der Registry.

use super::Collection;
use crate::model::{Activity, Kind};
use crate::reg::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, Key};
use chrono::{DateTime, Local, NaiveDate, TimeZone, Utc};

const UNINSTALL: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall";
const UNINSTALL_WOW64: &str = r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall";

pub fn collect(out: &mut Collection) {
    for (root, root_name, path) in [
        (HKEY_LOCAL_MACHINE, "HKLM", UNINSTALL),
        (HKEY_LOCAL_MACHINE, "HKLM", UNINSTALL_WOW64),
        (HKEY_CURRENT_USER, "HKCU", UNINSTALL),
    ] {
        let Some(key) = Key::open(root, path) else {
            continue;
        };
        for (sub, written) in key.subkeys() {
            let Some(display_name) = key.string(&sub, "DisplayName") else {
                continue;
            };
            let Some(time) = install_time(key.string(&sub, "InstallDate").as_deref(), written)
            else {
                continue;
            };
            let details = [
                key.string(&sub, "DisplayVersion"),
                key.string(&sub, "Publisher"),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · ");
            out.activities.push(Activity {
                time,
                kind: Kind::SoftwareInstalled,
                description: display_name,
                details,
                source: format!(r"{root_name}\{path}\{sub}"),
                extension: String::new(),
            });
        }
    }
}

/// InstallDate (JJJJMMTT) hat nur das Datum. Passt die Schreibzeit des Schlüssels zum selben Tag,
/// nehmen wir sie als genaue Uhrzeit, sonst Mitternacht des Installationstags.
fn install_time(
    install_date: Option<&str>,
    written: Option<DateTime<Utc>>,
) -> Option<DateTime<Utc>> {
    let Some(date) = install_date.and_then(|d| NaiveDate::parse_from_str(d.trim(), "%Y%m%d").ok())
    else {
        return written;
    };
    if let Some(w) = written.filter(|w| w.with_timezone(&Local).date_naive() == date) {
        return Some(w);
    }
    Local
        .from_local_datetime(&date.and_hms_opt(0, 0, 0)?)
        .earliest()
        .map(|t| t.with_timezone(&Utc))
}
