//! Installierte Software aus den Uninstall-Schlüsseln der Registry.

use super::Collection;
use crate::model::{Activity, Kind};
use crate::util::{filetime_to_utc, from_wide, wide};
use chrono::{DateTime, Local, NaiveDate, TimeZone, Utc};
use windows::Win32::Foundation::{ERROR_SUCCESS, FILETIME};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, RRF_RT_REG_SZ, RegCloseKey,
    RegEnumKeyExW, RegGetValueW, RegOpenKeyExW,
};
use windows::core::{PCWSTR, PWSTR};

const UNINSTALL: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall";
const UNINSTALL_WOW64: &str = r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall";

pub fn collect(out: &mut Collection) {
    for (root, root_name, path) in [
        (HKEY_LOCAL_MACHINE, "HKLM", UNINSTALL),
        (HKEY_LOCAL_MACHINE, "HKLM", UNINSTALL_WOW64),
        (HKEY_CURRENT_USER, "HKCU", UNINSTALL),
    ] {
        read_root(root, root_name, path, out);
    }
}

fn read_root(root: HKEY, root_name: &str, path: &str, out: &mut Collection) {
    let path_w = wide(path);
    let mut key = HKEY::default();
    if unsafe { RegOpenKeyExW(root, PCWSTR(path_w.as_ptr()), None, KEY_READ, &mut key) }
        != ERROR_SUCCESS
    {
        return;
    }
    for index in 0.. {
        let mut name = [0u16; 256];
        let mut name_len = name.len() as u32;
        let mut written = FILETIME::default();
        let status = unsafe {
            RegEnumKeyExW(
                key,
                index,
                Some(PWSTR(name.as_mut_ptr())),
                &mut name_len,
                None,
                None,
                None,
                Some(&mut written),
            )
        };
        if status != ERROR_SUCCESS {
            break;
        }
        let sub = from_wide(&name);
        let Some(display_name) = string_value(key, &sub, "DisplayName") else {
            continue;
        };
        let written =
            filetime_to_utc(((written.dwHighDateTime as u64) << 32) | written.dwLowDateTime as u64);
        let Some(time) = install_time(string_value(key, &sub, "InstallDate").as_deref(), written)
        else {
            continue;
        };
        let version = string_value(key, &sub, "DisplayVersion").unwrap_or_default();
        let publisher = string_value(key, &sub, "Publisher").unwrap_or_default();
        let details = [version, publisher]
            .into_iter()
            .filter(|s| !s.is_empty())
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
    unsafe {
        let _ = RegCloseKey(key);
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

fn string_value(key: HKEY, sub: &str, value: &str) -> Option<String> {
    let (sub_w, value_w) = (wide(sub), wide(value));
    let mut size = 0u32;
    let status = unsafe {
        RegGetValueW(
            key,
            PCWSTR(sub_w.as_ptr()),
            PCWSTR(value_w.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            None,
            Some(&mut size),
        )
    };
    if status != ERROR_SUCCESS || size == 0 {
        return None;
    }
    let mut buf = vec![0u16; (size as usize).div_ceil(2)];
    let status = unsafe {
        RegGetValueW(
            key,
            PCWSTR(sub_w.as_ptr()),
            PCWSTR(value_w.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    (status == ERROR_SUCCESS)
        .then(|| from_wide(&buf).trim().to_string())
        .filter(|s| !s.is_empty())
}
