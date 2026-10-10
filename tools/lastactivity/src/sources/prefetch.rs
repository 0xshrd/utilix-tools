//! Prefetch (C:\Windows\Prefetch): wann welche Programme gestartet wurden.
//! Ab Windows 10 sind die .pf-Dateien mit Xpress-Huffman komprimiert ("MAM"-Header).

use super::Collection;
use crate::model::{Activity, Kind};
use crate::util::{filetime_to_utc, from_wide, u32_at, u64_at, utf16_z, wide};
use std::collections::HashMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use windows::Wdk::Storage::FileSystem::{RtlDecompressBufferEx, RtlGetCompressionWorkSpaceSize};
use windows::Win32::Storage::FileSystem::{
    GetLogicalDrives, GetVolumeInformationW, QueryDosDeviceW,
};
use windows::core::PCWSTR;

const COMPRESSION_FORMAT_XPRESS_HUFF: u16 = 4;

pub fn collect(out: &mut Collection) {
    let dir = PathBuf::from(std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into()))
        .join("Prefetch");
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == ErrorKind::PermissionDenied => {
            out.warnings
                .push("Prefetch: Für gestartete Programme sind Administratorrechte nötig.".into());
            return;
        }
        Err(e) => {
            out.warnings.push(format!("Prefetch: {e}"));
            return;
        }
    };

    let drives = Drives::detect();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("pf"))
        {
            continue;
        }
        let Some(info) = fs::read(&path)
            .ok()
            .and_then(|raw| decompress(&raw))
            .and_then(|d| parse(&d, expected_hash(&path), &drives))
        else {
            continue;
        };
        let source = path.display().to_string();
        let details = format!("{} Ausführungen insgesamt", info.run_count);
        for time in info.run_times.iter().filter_map(|&ft| filetime_to_utc(ft)) {
            out.activities.push(Activity {
                time,
                kind: Kind::ProgramRun,
                description: info.path.clone(),
                details: details.clone(),
                source: source.clone(),
                extension: "exe".into(),
            });
        }
    }
}

struct PrefetchInfo {
    path: String,
    run_times: Vec<u64>,
    run_count: u32,
}

/// Laufwerksbuchstaben dieses Rechners, auffindbar über Seriennummer oder Gerätepfad.
struct Drives {
    by_serial: HashMap<u32, String>,
    by_device: Vec<(String, String)>,
}

impl Drives {
    fn detect() -> Self {
        let mut drives = Drives {
            by_serial: HashMap::new(),
            by_device: Vec::new(),
        };
        let mask = unsafe { GetLogicalDrives() };
        for i in 0..26u8 {
            if mask & (1 << i) == 0 {
                continue;
            }
            let letter = format!("{}:", (b'A' + i) as char);
            let root = wide(&format!("{letter}\\"));
            let mut serial = 0u32;
            if unsafe {
                GetVolumeInformationW(
                    PCWSTR(root.as_ptr()),
                    None,
                    Some(&mut serial),
                    None,
                    None,
                    None,
                )
            }
            .is_ok()
            {
                drives.by_serial.insert(serial, letter.clone());
            }
            let name = wide(&letter);
            let mut target = [0u16; 512];
            if unsafe { QueryDosDeviceW(PCWSTR(name.as_ptr()), Some(&mut target)) } > 0 {
                drives
                    .by_device
                    .push((from_wide(&target).to_uppercase(), letter));
            }
        }
        drives
    }
}

/// Der Dateiname "PROGRAMM.EXE-1A2B3C4D.pf" enthält den Hash des vollständigen Gerätepfads.
fn expected_hash(pf: &Path) -> Option<u32> {
    let stem = pf.file_stem()?.to_str()?;
    u32::from_str_radix(stem.rsplit_once('-')?.1, 16).ok()
}

/// Prefetch-Hash ab Windows Vista ("SCCA 2008"), über den großgeschriebenen UTF-16-Gerätepfad.
fn scca_hash(device_path: &str) -> u32 {
    let bytes: Vec<u8> = device_path
        .to_uppercase()
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    let mut hash: u32 = 314_159;
    let mut i = 0;
    while i + 8 < bytes.len() {
        let b = |k: usize| u32::from(bytes[i + k]);
        let mut v = b(1).wrapping_mul(37);
        for k in 2..=6 {
            v = v.wrapping_add(b(k)).wrapping_mul(37);
        }
        v = v
            .wrapping_add(b(0).wrapping_mul(442_596_621))
            .wrapping_add(b(7));
        hash = v.wrapping_sub(hash.wrapping_mul(803_794_207));
        i += 8;
    }
    for &byte in &bytes[i..] {
        hash = hash.wrapping_mul(37).wrapping_add(u32::from(byte));
    }
    hash
}

fn decompress(raw: &[u8]) -> Option<Vec<u8>> {
    if raw.get(0..3)? != b"MAM" {
        return Some(raw.to_vec());
    }
    let flags = *raw.get(3)?;
    if flags & 0x0F != COMPRESSION_FORMAT_XPRESS_HUFF as u8 {
        return None;
    }
    // Bit 7 gesetzt: nach der Größe folgt eine 4-Byte-Prüfsumme.
    let start = if flags & 0x80 != 0 { 12 } else { 8 };
    let size = u32_at(raw, 4)? as usize;

    let (mut ws_compress, mut ws_fragment) = (0u32, 0u32);
    unsafe {
        RtlGetCompressionWorkSpaceSize(
            COMPRESSION_FORMAT_XPRESS_HUFF,
            &mut ws_compress,
            &mut ws_fragment,
        )
    }
    .ok()
    .ok()?;
    let workspace = vec![0u8; ws_fragment as usize];
    let mut out = vec![0u8; size];
    let mut final_size = 0u32;
    unsafe {
        RtlDecompressBufferEx(
            COMPRESSION_FORMAT_XPRESS_HUFF,
            &mut out,
            raw.get(start..)?,
            &mut final_size,
            Some(workspace.as_ptr().cast()),
        )
    }
    .ok()
    .ok()?;
    out.truncate(final_size as usize);
    Some(out)
}

fn parse(d: &[u8], expected: Option<u32>, drives: &Drives) -> Option<PrefetchInfo> {
    if d.get(4..8)? != b"SCCA" {
        return None;
    }
    let version = u32_at(d, 0)?;
    let name = utf16_z(d.get(0x10..0x10 + 60)?, 0)?;

    let (time_offsets, count_offset, volume_entry_size): (Vec<usize>, usize, usize) = match version
    {
        17 => (vec![0x78], 0x90, 40),
        23 => (vec![0x80], 0x98, 104),
        26 => ((0..8).map(|i| 0x80 + i * 8).collect(), 0xD0, 104),
        // Version 30 gibt es mit zwei Headergrößen; erkennbar am Beginn des ersten Abschnitts.
        30 | 31 => {
            let count = if u32_at(d, 0x54)? == 0x128 {
                0xC8
            } else {
                0xD0
            };
            ((0..8).map(|i| 0x80 + i * 8).collect(), count, 96)
        }
        _ => return None,
    };
    let run_times = time_offsets.iter().filter_map(|&o| u64_at(d, o)).collect();
    let run_count = u32_at(d, count_offset).unwrap_or(0);
    let volumes = volumes(d, volume_entry_size).unwrap_or_default();

    Some(PrefetchInfo {
        path: resolve_path(d, &name, expected, &volumes, drives).unwrap_or(name),
        run_times,
        run_count,
    })
}

/// Gerätepfade und Seriennummern der Volumes, die das Programm benutzt hat.
fn volumes(d: &[u8], entry_size: usize) -> Option<Vec<(String, u32)>> {
    let section = u32_at(d, 0x6C)? as usize;
    let count = u32_at(d, 0x70)? as usize;
    (0..count.min(32))
        .map(|i| {
            let entry = section + i * entry_size;
            let path_offset = u32_at(d, entry)? as usize;
            let path_len = u32_at(d, entry + 4)? as usize;
            let units: Vec<u16> = d
                .get(section + path_offset..section + path_offset + path_len * 2)?
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| u16::from_le_bytes(*c))
                .collect();
            Some((
                String::from_utf16_lossy(&units).to_uppercase(),
                u32_at(d, entry + 16)?,
            ))
        })
        .collect()
}

/// Sucht den Pfad der .exe in der Dateinamen-Liste. Bei mehreren Treffern entscheidet der Hash
/// im Dateinamen. Ergebnis mit Laufwerksbuchstabe und echter Schreibweise, soweit möglich.
fn resolve_path(
    d: &[u8],
    name: &str,
    expected: Option<u32>,
    volumes: &[(String, u32)],
    drives: &Drives,
) -> Option<String> {
    let offset = u32_at(d, 0x64)? as usize;
    let size = u32_at(d, 0x68)? as usize;
    let units: Vec<u16> = d
        .get(offset..offset + size)?
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .collect();
    // Prefetch kürzt den Programmnamen auf 29 Zeichen; dann zählt nur der Anfang.
    let name = name.to_uppercase();
    let truncated = name.chars().count() >= 29;
    let candidates: Vec<String> = units
        .split(|&c| c == 0)
        .map(String::from_utf16_lossy)
        .filter(|s| {
            let file = s.rsplit('\\').next().unwrap_or("").to_uppercase();
            file == name || (truncated && file.starts_with(&name))
        })
        .collect();

    let chosen = candidates
        .iter()
        .find(|c| {
            expected.is_some_and(|h| device_path(c, volumes).is_some_and(|p| scca_hash(&p) == h))
        })
        .or_else(|| candidates.first())?;

    Some(match drive_path(chosen, drives) {
        Some(path) => real_case(&path).unwrap_or(path),
        None => strip_volume(chosen),
    })
}

/// "\VOLUME{zeit-SERIAL}\…" → "\DEVICE\HARDDISKVOLUMEx\…" anhand der Volume-Liste der Datei.
fn device_path(path: &str, volumes: &[(String, u32)]) -> Option<String> {
    if path.to_uppercase().starts_with(r"\DEVICE\") {
        return Some(path.to_string());
    }
    let (serial, rest) = split_volume(path)?;
    let (device, _) = volumes.iter().find(|(_, s)| *s == serial)?;
    Some(format!("{device}{rest}"))
}

/// Gleicher Pfad mit Laufwerksbuchstaben dieses Rechners.
fn drive_path(path: &str, drives: &Drives) -> Option<String> {
    if let Some((serial, rest)) = split_volume(path) {
        return drives
            .by_serial
            .get(&serial)
            .map(|letter| format!("{letter}{rest}"));
    }
    let upper = path.to_uppercase();
    drives.by_device.iter().find_map(|(device, letter)| {
        upper
            .strip_prefix(device.as_str())
            .filter(|rest| rest.starts_with('\\'))
            .map(|_| format!("{letter}{}", &path[device.len()..]))
    })
}

/// "\VOLUME{01d1…-1a2b3c4d}\REST" → (0x1a2b3c4d, "\REST")
fn split_volume(path: &str) -> Option<(u32, &str)> {
    let inner = path.strip_prefix(r"\VOLUME{")?;
    let close = inner.find('}')?;
    let serial = u32::from_str_radix(inner[..close].rsplit_once('-')?.1, 16).ok()?;
    Some((serial, &inner[close + 1..]))
}

fn strip_volume(path: &str) -> String {
    match path
        .strip_prefix(r"\VOLUME{")
        .and_then(|r| r.find('}').map(|i| &r[i + 1..]))
    {
        Some(rest) => rest.to_string(),
        None => path.to_string(),
    }
}

/// Prefetch speichert alles in Großbuchstaben; existiert die Datei noch, holen wir die echte Schreibweise.
fn real_case(path: &str) -> Option<String> {
    let canonical = fs::canonicalize(path).ok()?;
    let s = canonical.to_string_lossy();
    Some(s.strip_prefix(r"\\?\").unwrap_or(&s).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_volume_path() {
        let (serial, rest) =
            split_volume(r"\VOLUME{01d9a1b2c3d4e5f6-6a2b1c3d}\WINDOWS\SYSTEM32\CMD.EXE").unwrap();
        assert_eq!(serial, 0x6a2b1c3d);
        assert_eq!(rest, r"\WINDOWS\SYSTEM32\CMD.EXE");
    }

    #[test]
    fn hash_is_case_insensitive() {
        assert_eq!(
            scca_hash(r"\DEVICE\HARDDISKVOLUME3\WINDOWS\SYSTEM32\CMD.EXE"),
            scca_hash(r"\Device\HarddiskVolume3\Windows\System32\cmd.exe")
        );
    }
}
