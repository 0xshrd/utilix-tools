mod eventlog;
mod opensave;
mod prefetch;
mod recent;
mod shellbags;
mod uninstall;

use crate::model::Activity;

#[derive(Default)]
pub struct Collection {
    pub activities: Vec<Activity>,
    /// Hinweise für die Statuszeile, z. B. fehlende Rechte.
    pub warnings: Vec<String>,
}

/// Liest alle Quellen parallel und liefert die Einträge neueste zuerst.
pub fn collect_all() -> Collection {
    let sources: [fn(&mut Collection); 6] = [
        eventlog::collect,
        prefetch::collect,
        recent::collect,
        shellbags::collect,
        opensave::collect,
        uninstall::collect,
    ];
    let parts: Vec<Collection> = std::thread::scope(|scope| {
        let handles: Vec<_> = sources
            .iter()
            .map(|source| {
                scope.spawn(move || {
                    let mut part = Collection::default();
                    source(&mut part);
                    part
                })
            })
            .collect();
        handles.into_iter().filter_map(|h| h.join().ok()).collect()
    });

    let mut all = Collection::default();
    for part in parts {
        all.activities.extend(part.activities);
        all.warnings.extend(part.warnings);
    }
    all.activities.sort_by_key(|a| std::cmp::Reverse(a.time));
    all
}
