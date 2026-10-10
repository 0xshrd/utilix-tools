//! ShellBags: Ordner, die im Explorer angesehen wurden. Der Explorer speichert sie als Baum
//! unter BagMRU; jeder Knoten hält ein Shell-Item, der Pfad ergibt sich aus dem Weg zur Wurzel.

use super::Collection;
use crate::model::{Activity, Kind};
use crate::reg::{HKEY_CURRENT_USER, Key};
use crate::shellitem::{self, Segment};
use std::collections::HashSet;

const ROOTS: [&str; 2] = [
    r"Software\Classes\Local Settings\Software\Microsoft\Windows\Shell\BagMRU",
    r"Software\Microsoft\Windows\Shell\BagMRU",
];
const MAX_DEPTH: usize = 64;

pub fn collect(out: &mut Collection) {
    let mut seen = HashSet::new();
    for root in ROOTS {
        if let Some(key) = Key::open(HKEY_CURRENT_USER, root) {
            walk(
                &key,
                &format!(r"HKCU\{root}"),
                &mut Vec::new(),
                &mut seen,
                out,
            );
        }
    }
}

fn walk(
    key: &Key,
    key_path: &str,
    segments: &mut Vec<Segment>,
    seen: &mut HashSet<(String, i64)>,
    out: &mut Collection,
) {
    if segments.len() >= MAX_DEPTH {
        return;
    }
    for (name, written) in key.subkeys() {
        if name.parse::<u32>().is_err() {
            continue;
        }
        let Some(data) = key.binary(&name) else {
            continue;
        };
        let Some(item) = shellitem::split_pidl(&data).first().copied() else {
            continue;
        };
        segments.push(shellitem::parse(item));
        let path = shellitem::join(segments);
        let sub_path = format!(r"{key_path}\{name}");

        // Die Schreibzeit des Knotens ist der beste verfügbare Hinweis auf den letzten Besuch.
        if let Some(time) = written
            && !path.is_empty()
            && seen.insert((path.clone(), time.timestamp()))
        {
            out.activities.push(Activity {
                time,
                kind: Kind::FolderViewed,
                description: path,
                details: "Zeitpunkt aus der Registry, kann ungenau sein".into(),
                source: sub_path.clone(),
                extension: String::new(),
            });
        }
        if let Some(sub) = key.open_sub(&name) {
            walk(&sub, &sub_path, segments, seen, out);
        }
        segments.pop();
    }
}
