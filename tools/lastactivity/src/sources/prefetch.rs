//! Prefetch (C:\Windows\Prefetch): wann welche Programme gestartet wurden.
//! Ab Windows 10 sind die .pf-Dateien mit Xpress-Huffman komprimiert ("MAM"-Header).

use super::Collection;
use crate::model::{Activity, Kind};
use crate::util::{filetime_to_utc, u32_at, u64_at, utf16_z};
use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;
use windows::Wdk::Storage::FileSystem::{RtlDecompressBufferEx, RtlGetCompressionWorkSpaceSize};

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
            .and_then(|d| parse(&d))
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

fn parse(d: &[u8]) -> Option<PrefetchInfo> {
    if d.get(4..8)? != b"SCCA" {
        return None;
    }
    let version = u32_at(d, 0)?;
    let name = utf16_z(d.get(0x10..0x10 + 60)?, 0)?;

    let (time_offsets, count_offset): (Vec<usize>, usize) = match version {
        17 => (vec![0x78], 0x90),
        23 => (vec![0x80], 0x98),
        26 => ((0..8).map(|i| 0x80 + i * 8).collect(), 0xD0),
        // Version 30 gibt es mit zwei Headergrößen; erkennbar am Beginn des ersten Abschnitts.
        30 | 31 => {
            let count = if u32_at(d, 0x54)? == 0x128 {
                0xC8
            } else {
                0xD0
            };
            ((0..8).map(|i| 0x80 + i * 8).collect(), count)
        }
        _ => return None,
    };
    let run_times = time_offsets.iter().filter_map(|&o| u64_at(d, o)).collect();
    let run_count = u32_at(d, count_offset).unwrap_or(0);

    Some(PrefetchInfo {
        path: full_path(d, &name).unwrap_or(name),
        run_times,
        run_count,
    })
}

/// Vollständigen Pfad der .exe aus der Dateinamen-Liste suchen.
/// Prefetch speichert ihn als "\VOLUME{…}\PFAD"; den Volume-Teil lassen wir weg.
fn full_path(d: &[u8], name: &str) -> Option<String> {
    let offset = u32_at(d, 0x64)? as usize;
    let size = u32_at(d, 0x68)? as usize;
    let units: Vec<u16> = d
        .get(offset..offset + size)?
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .collect();
    let suffix = format!("\\{}", name.to_uppercase());
    units
        .split(|&c| c == 0)
        .map(String::from_utf16_lossy)
        .find(|s| s.to_uppercase().ends_with(&suffix))
        .map(|s| {
            match s
                .strip_prefix(r"\VOLUME{")
                .and_then(|r| r.find('}').map(|i| &r[i + 1..]))
            {
                Some(rest) => rest.to_string(),
                None => s,
            }
        })
}
