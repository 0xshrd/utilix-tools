use chrono::{DateTime, TimeZone, Utc};
use std::path::Path;

/// Windows-FILETIME (100-ns-Schritte seit 1601) nach UTC.
pub fn filetime_to_utc(ft: u64) -> Option<DateTime<Utc>> {
    if ft == 0 {
        return None;
    }
    let secs = (ft / 10_000_000) as i64 - 11_644_473_600;
    let nanos = ((ft % 10_000_000) * 100) as u32;
    Utc.timestamp_opt(secs, nanos).single()
}

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

pub fn from_wide(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

pub fn u16_at(b: &[u8], off: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(off..off + 2)?.try_into().ok()?))
}

pub fn u32_at(b: &[u8], off: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(off..off + 4)?.try_into().ok()?))
}

pub fn u64_at(b: &[u8], off: usize) -> Option<u64> {
    Some(u64::from_le_bytes(b.get(off..off + 8)?.try_into().ok()?))
}

/// Nullterminierter UTF-16-String ab `off`.
pub fn utf16_z(b: &[u8], off: usize) -> Option<String> {
    let units: Vec<u16> = b
        .get(off..)?
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .take_while(|&c| c != 0)
        .collect();
    Some(String::from_utf16_lossy(&units))
}

/// Nullterminierter 8-Bit-String ab `off` (Latin-1, deckt Umlaute ab).
pub fn latin1_z(b: &[u8], off: usize) -> Option<String> {
    Some(
        b.get(off..)?
            .iter()
            .take_while(|&&c| c != 0)
            .map(|&c| c as char)
            .collect(),
    )
}

pub fn extension_of(path: &str) -> String {
    Path::new(path)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}
