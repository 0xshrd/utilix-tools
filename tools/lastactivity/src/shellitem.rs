//! Shell-Items und PIDLs: das Binärformat, in dem der Explorer Pfade in ShellBags und
//! Dialog-Verläufen speichert. Wir lesen nur, was für einen lesbaren Pfad nötig ist.

use crate::util::{latin1_z, u16_at, u32_at};

#[derive(Clone, Debug, PartialEq)]
pub enum Segment {
    /// Virtueller Startpunkt wie "Dieser PC" oder ein bekannter Ordner.
    Root(String),
    /// Laufwerk, z. B. "C:\".
    Volume(String),
    /// Netzwerkpfad, z. B. "\\server\freigabe".
    Network(String),
    /// Ordner- oder Dateiname.
    Name(String),
    Unknown,
}

/// Zerlegt eine PIDL (Folge von Items mit 2-Byte-Längenpräfix, Ende = 0) in ihre Items.
pub fn split_pidl(data: &[u8]) -> Vec<&[u8]> {
    let mut items = Vec::new();
    let mut off = 0;
    while let Some(size) = u16_at(data, off).map(usize::from) {
        if size < 3 || off + size > data.len() {
            break;
        }
        items.push(&data[off..off + size]);
        off += size;
    }
    items
}

pub fn parse(item: &[u8]) -> Segment {
    let Some(&kind) = item.get(2) else {
        return Segment::Unknown;
    };
    if let Some(name) = zip_entry(item) {
        return Segment::Name(name);
    }
    match kind {
        // Wechseldatenträger tauchen auch als Root-Item mit eingebettetem Volume (0x2F) auf.
        0x1F if item.get(4) == Some(&0x2F) && drive_letter(item).is_some() => {
            drive_letter(item).map_or(Segment::Unknown, Segment::Volume)
        }
        0x1F => guid_at(item, 4).map_or(Segment::Unknown, |g| Segment::Root(known_folder(&g))),
        0x20..=0x2F => drive_letter(item).map_or_else(
            || guid_at(item, 4).map_or(Segment::Unknown, |g| Segment::Root(known_folder(&g))),
            Segment::Volume,
        ),
        0x30..=0x3F => file_entry(item).map_or(Segment::Unknown, Segment::Name),
        0x40..=0x4F => latin1_z(item, 5)
            .filter(|s| !s.is_empty())
            .map_or(Segment::Unknown, Segment::Network),
        0x71 => Segment::Root("Systemsteuerung".into()),
        // Delegate-Items (z. B. Benutzerordner, OneDrive) enthalten oft dieselbe Namenserweiterung.
        _ => long_name(item, None).map_or(Segment::Unknown, Segment::Name),
    }
}

/// Setzt Segmente zu einem lesbaren Pfad zusammen.
pub fn join(segments: &[Segment]) -> String {
    let mut path = String::new();
    for segment in segments {
        match segment {
            // "Dieser PC" verschwindet, sobald ein Laufwerk folgt.
            Segment::Root(name) if path.is_empty() => path = name.clone(),
            Segment::Volume(v) | Segment::Network(v) => path = v.clone(),
            Segment::Root(name) | Segment::Name(name) => push_part(&mut path, name),
            Segment::Unknown => push_part(&mut path, "…"),
        }
    }
    path
}

/// Laufwerksbuchstabe "X:\". Meist ab Offset 3, bei Wechseldatenträgern weiter hinten.
fn drive_letter(item: &[u8]) -> Option<String> {
    let head = item.get(3..item.len().min(40))?;
    head.windows(3)
        .find(|w| w[0].is_ascii_alphabetic() && w[1] == b':' && w[2] == b'\\')
        .map(|w| format!("{}:\\", (w[0] as char).to_ascii_uppercase()))
}

fn push_part(path: &mut String, part: &str) {
    if !path.is_empty() && !path.ends_with('\\') {
        path.push('\\');
    }
    path.push_str(part);
}

/// Ordner innerhalb eines ZIP-Archivs: Datum als Text ab 0x24 ("MM/TT/JJJJ …" oder "N/A"),
/// Länge des Namens bei 0x54, Name (UTF-16) ab 0x5C. Das Typ-Byte ist hier nicht aussagekräftig.
fn zip_entry(item: &[u8]) -> Option<String> {
    let date: Vec<u16> = item
        .get(0x24..0x2A)?
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .collect();
    let looks_like_date =
        date.get(2) == Some(&(b'/' as u16)) || date == [b'N' as u16, b'/' as u16, b'A' as u16];
    if !looks_like_date {
        return None;
    }
    let len = u32_at(item, 0x54)? as usize;
    let units: Vec<u16> = item
        .get(0x5C..0x5C + len.checked_mul(2)?)?
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .collect();
    String::from_utf16(&units).ok().filter(|s| !s.is_empty())
}

fn file_entry(item: &[u8]) -> Option<String> {
    let short = latin1_z(item, 14).unwrap_or_default();
    Some(long_name(item, Some(&short)).unwrap_or(short)).filter(|s| !s.is_empty())
}

/// Langer Name aus der Erweiterung 0xBEEF0004. Ihr Aufbau hängt von der Windows-Version ab,
/// daher probieren wir die bekannten Offsets und prüfen das Ergebnis gegen den kurzen 8.3-Namen.
fn long_name(item: &[u8], short: Option<&str>) -> Option<String> {
    let sig = item
        .windows(4)
        .position(|w| w == [0x04, 0x00, 0xEF, 0xBE])?;
    let start = sig.checked_sub(4)?;
    let size = u16_at(item, start)? as usize;
    let block = item.get(start..start + size)?;
    let version = u16_at(block, 2)?;

    let preferred = match version {
        9.. => 46,
        8 => 42,
        7 => 38,
        _ => 20,
    };
    let candidates: Vec<String> = [preferred, 46, 42, 38, 50, 20]
        .into_iter()
        .filter_map(|off| utf16_in_block(block, off))
        .collect();
    candidates
        .iter()
        .find(|name| short.is_some_and(|s| matches_short(name, s)))
        .or_else(|| candidates.first())
        .cloned()
}

fn utf16_in_block(block: &[u8], off: usize) -> Option<String> {
    let mut units = Vec::new();
    let mut pos = off;
    loop {
        let unit = u16_at(block, pos)?;
        if unit == 0 {
            break;
        }
        units.push(unit);
        pos += 2;
    }
    let name = String::from_utf16(&units).ok()?;
    let valid = !name.is_empty() && !name.chars().any(|c| c.is_control() || c == '\u{fffd}');
    valid.then_some(name)
}

fn matches_short(long: &str, short: &str) -> bool {
    let (long, short) = (long.to_uppercase(), short.to_uppercase());
    match short.split_once('~') {
        Some((prefix, _)) => long
            .replace([' ', '.'], "")
            .starts_with(&prefix.replace('.', "")),
        None => long == short,
    }
}

fn guid_at(b: &[u8], off: usize) -> Option<String> {
    let d1 = u32_at(b, off)?;
    let d2 = u16_at(b, off + 4)?;
    let d3 = u16_at(b, off + 6)?;
    let d4 = b.get(off + 8..off + 16)?;
    Some(format!(
        "{d1:08X}-{d2:04X}-{d3:04X}-{:02X}{:02X}-{}",
        d4[0],
        d4[1],
        d4[2..]
            .iter()
            .map(|x| format!("{x:02X}"))
            .collect::<String>()
    ))
}

fn known_folder(guid: &str) -> String {
    let user = || std::env::var("USERPROFILE").unwrap_or_else(|_| "Benutzerordner".into());
    match guid {
        "20D04FE0-3AEA-1069-A2D8-08002B30309D" => "Dieser PC".into(),
        "208D2C60-3AEA-1069-A2D7-08002B30309D" | "F02C1A0D-BE21-4350-88B0-7367FC96EF3C" => {
            "Netzwerk".into()
        }
        "59031A47-3F72-44A7-89C5-5595FE6B30EE" => user(),
        "B4BFCC3A-DB2C-424C-B029-7FE99A87C641" => format!(r"{}\Desktop", user()),
        "D3162B92-9365-467A-956B-92703ACA08AF" => format!(r"{}\Documents", user()),
        "088E3905-0323-4B02-9826-5D99428E115F" => format!(r"{}\Downloads", user()),
        "3DFDF296-DBEC-4FB4-81D1-6A3438BCF4DE" => format!(r"{}\Music", user()),
        "24AD3AD4-A569-4530-98E1-AB02F9417AA8" => format!(r"{}\Pictures", user()),
        "F86FA3AB-70D2-4FC7-9C99-FCBF05467F3A" => format!(r"{}\Videos", user()),
        // Ordner unter "Dieser PC" (Windows 8 und neuer)
        "A8CDFF1C-4878-43BE-B5FD-F8091C1C60D0" => format!(r"{}\Documents", user()),
        "374DE290-123F-4565-9164-39C4925E467B" => format!(r"{}\Downloads", user()),
        "1CF1260C-4DD0-4EBB-811F-33C572699FDE" => format!(r"{}\Music", user()),
        "3ADD1653-EB32-4CB0-BBD7-DFA0ABB5ACCA" => format!(r"{}\Pictures", user()),
        "A0953C92-50DC-43BF-BE83-3742FED03C9C" => format!(r"{}\Videos", user()),
        "E88865EA-0E1C-4E20-9AA6-EDCD0212C87C" => "Galerie".into(),
        "018D5C66-4533-4307-9B53-224DE2ED1FE6" => "OneDrive".into(),
        "031E4825-7B94-4DC3-B131-E946B44C8DD5" => "Bibliotheken".into(),
        "679F85CB-0220-4080-B29B-5540CC05AAB6" => "Schnellzugriff".into(),
        "F874310E-B6B7-47DC-BC84-B9E6B38F5903" => "Start".into(),
        "645FF040-5081-101B-9F08-00AA002F954E" => "Papierkorb".into(),
        "26EE0668-A00A-44D7-9371-BEB064C98683" | "21EC2020-3AEA-1069-A2DD-08002B30309D" => {
            "Systemsteuerung".into()
        }
        other => format!("{{{other}}}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joins_volume_and_names() {
        let path = join(&[
            Segment::Root("Dieser PC".into()),
            Segment::Volume(r"C:\".into()),
            Segment::Name("Users".into()),
            Segment::Name("aaron".into()),
        ]);
        assert_eq!(path, r"C:\Users\aaron");
    }

    #[test]
    fn short_name_matching() {
        assert!(matches_short("Program Files", "PROGRA~1"));
        assert!(matches_short("Users", "Users"));
        assert!(!matches_short("Windows", "Users"));
    }

    #[test]
    fn parses_root_guid() {
        // Item: Größe 0x14, Typ 0x1F, Sortierindex, GUID von "Dieser PC"
        let mut item = vec![0x14, 0x00, 0x1F, 0x50];
        item.extend([
            0xE0, 0x4F, 0xD0, 0x20, 0xEA, 0x3A, 0x69, 0x10, 0xA2, 0xD8, 0x08, 0x00, 0x2B, 0x30,
            0x30, 0x9D,
        ]);
        assert_eq!(parse(&item), Segment::Root("Dieser PC".into()));
    }
}
