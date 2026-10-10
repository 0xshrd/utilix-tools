use crate::export::{self, Format};
use crate::model::{Activity, Kind};
use crate::sources::{self, Collection};
use crate::util::wide;
use chrono::{DateTime, Duration, Local, Utc};
use eframe::egui::{self, Color32, RichText};
use egui_extras::{Column, TableBuilder};
use std::collections::HashSet;
use std::sync::mpsc::{self, Receiver};

const ACCENT: Color32 = Color32::from_rgb(0xBF, 0xCF, 0x5B);
const PROBLEM: Color32 = Color32::from_rgb(0xFF, 0x8A, 0x65);
const MUTED: Color32 = Color32::from_rgb(0x9F, 0xB0, 0x9A);

pub fn run() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("LastActivity · utilix")
            .with_inner_size([1280.0, 760.0])
            .with_min_inner_size([720.0, 400.0])
            .with_icon(
                eframe::icon_data::from_png_bytes(include_bytes!("../../../assets/icon.png"))
                    .unwrap_or_default(),
            ),
        ..Default::default()
    };
    eframe::run_native(
        "LastActivity",
        options,
        Box::new(|cc| Ok(Box::new(App::new(&cc.egui_ctx)))),
    )
}

/// Grobe Kategorien als Schnellfilter; die Feinauswahl erfolgt über "Aktionen".
#[derive(Clone, Copy, PartialEq, Eq)]
enum Group {
    All,
    Programs,
    Files,
    System,
    Network,
    Problems,
}

impl Group {
    const ALL: [Group; 6] = [
        Group::All,
        Group::Programs,
        Group::Files,
        Group::System,
        Group::Network,
        Group::Problems,
    ];

    fn label(self) -> &'static str {
        match self {
            Group::All => "Alle Kategorien",
            Group::Programs => "Programme",
            Group::Files => "Dateien & Ordner",
            Group::System => "System",
            Group::Network => "Netzwerk",
            Group::Problems => "Probleme",
        }
    }

    fn contains(self, kind: Kind) -> bool {
        use Kind::*;
        match self {
            Group::All => true,
            Group::Programs => matches!(
                kind,
                ProgramRun | SoftwareInstalled | InstallerStarted | InstallerEnded | TaskRun
            ),
            Group::Files => matches!(
                kind,
                FileOpened | FolderOpened | FolderViewed | FileSelected
            ),
            Group::System => matches!(
                kind,
                SystemStart | SystemShutdown | Sleep | Resume | Logon | Logoff | RestorePoint
            ),
            Group::Network => matches!(
                kind,
                NetworkConnected | NetworkDisconnected | WlanConnected | WlanDisconnected
            ),
            Group::Problems => kind.is_problem(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Range {
    All,
    Hour,
    Day,
    Week,
    Month,
}

impl Range {
    const ALL: [Range; 5] = [
        Range::All,
        Range::Hour,
        Range::Day,
        Range::Week,
        Range::Month,
    ];

    fn label(self) -> &'static str {
        match self {
            Range::All => "Gesamter Zeitraum",
            Range::Hour => "Letzte Stunde",
            Range::Day => "Letzte 24 Stunden",
            Range::Week => "Letzte 7 Tage",
            Range::Month => "Letzte 30 Tage",
        }
    }

    fn duration(self) -> Option<Duration> {
        match self {
            Range::All => None,
            Range::Hour => Some(Duration::hours(1)),
            Range::Day => Some(Duration::days(1)),
            Range::Week => Some(Duration::days(7)),
            Range::Month => Some(Duration::days(30)),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SortKey {
    Time,
    Kind,
    Description,
    Source,
}

struct App {
    data: Vec<Activity>,
    warnings: Vec<String>,
    loading: Option<Receiver<Collection>>,
    loaded_at: Option<DateTime<Local>>,
    query: String,
    group: Group,
    range: Range,
    hidden: HashSet<Kind>,
    sort: SortKey,
    ascending: bool,
    /// Indizes in `data`, gefiltert und sortiert.
    visible: Vec<usize>,
    dirty: bool,
    selected: Option<usize>,
    status: Option<String>,
    is_admin: bool,
}

impl App {
    fn new(ctx: &egui::Context) -> Self {
        apply_style(ctx);
        let mut app = App {
            data: Vec::new(),
            warnings: Vec::new(),
            loading: None,
            loaded_at: None,
            query: String::new(),
            group: Group::All,
            range: Range::All,
            hidden: HashSet::new(),
            sort: SortKey::Time,
            ascending: false,
            visible: Vec::new(),
            dirty: false,
            selected: None,
            status: None,
            is_admin: unsafe { windows::Win32::UI::Shell::IsUserAnAdmin().as_bool() },
        };
        app.reload(ctx);
        app
    }

    fn reload(&mut self, ctx: &egui::Context) {
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(sources::collect_all());
            ctx.request_repaint();
        });
        self.loading = Some(rx);
        self.selected = None;
    }

    fn refilter(&mut self) {
        let query = self.query.to_lowercase();
        let cutoff = self.range.duration().map(|d| Utc::now() - d);
        let data = &self.data;
        self.visible = (0..data.len())
            .filter(|&i| {
                let a = &data[i];
                self.group.contains(a.kind)
                    && !self.hidden.contains(&a.kind)
                    && cutoff.is_none_or(|c| a.time >= c)
                    && (query.is_empty()
                        || [a.kind.label(), &a.description, &a.details, &a.source]
                            .iter()
                            .any(|s| s.to_lowercase().contains(&query)))
            })
            .collect();

        let key = self.sort;
        self.visible.sort_by(|&x, &y| {
            let (a, b) = (&data[x], &data[y]);
            let order = match key {
                SortKey::Time => a.time.cmp(&b.time),
                SortKey::Kind => a.kind.label().cmp(b.kind.label()).then(a.time.cmp(&b.time)),
                SortKey::Description => a
                    .description
                    .to_lowercase()
                    .cmp(&b.description.to_lowercase()),
                SortKey::Source => a.source.cmp(&b.source),
            };
            if self.ascending {
                order
            } else {
                order.reverse()
            }
        });
    }

    fn export(&mut self, format: Format) {
        let Some(path) = rfd::FileDialog::new()
            .set_file_name(format!("lastactivity.{}", format.extension()))
            .add_filter(format.label(), &[format.extension()])
            .save_file()
        else {
            return;
        };
        let rows: Vec<_> = self.visible.iter().map(|&i| &self.data[i]).collect();
        self.status = Some(match export::write(&path, format, &rows) {
            Ok(()) => format!("{} Einträge gespeichert: {}", rows.len(), path.display()),
            Err(e) => format!("Export fehlgeschlagen: {e}"),
        });
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("LastActivity")
                    .strong()
                    .size(16.0)
                    .color(ACCENT),
            );
            ui.add_space(8.0);

            let search = ui.add(
                egui::TextEdit::singleline(&mut self.query)
                    .hint_text("Suchen … (Strg+F)")
                    .desired_width(260.0),
            );
            if ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::F)) {
                search.request_focus();
            }
            if search.changed() {
                self.dirty = true;
            }
            if search.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.query.clear();
                self.dirty = true;
            }

            egui::ComboBox::from_id_salt("group")
                .selected_text(self.group.label())
                .show_ui(ui, |ui| {
                    for group in Group::ALL {
                        if ui
                            .selectable_value(&mut self.group, group, group.label())
                            .changed()
                        {
                            self.dirty = true;
                        }
                    }
                });

            egui::ComboBox::from_id_salt("range")
                .selected_text(self.range.label())
                .show_ui(ui, |ui| {
                    for range in Range::ALL {
                        if ui
                            .selectable_value(&mut self.range, range, range.label())
                            .changed()
                        {
                            self.dirty = true;
                        }
                    }
                });

            let shown = Kind::ALL.len() - self.hidden.len();
            ui.menu_button(format!("Aktionen ({shown}/{})", Kind::ALL.len()), |ui| {
                ui.horizontal(|ui| {
                    if ui.button("Alle").clicked() {
                        self.hidden.clear();
                        self.dirty = true;
                    }
                    if ui.button("Keine").clicked() {
                        self.hidden.extend(Kind::ALL);
                        self.dirty = true;
                    }
                });
                ui.separator();
                for kind in Kind::ALL {
                    let count = self.data.iter().filter(|a| a.kind == kind).count();
                    let mut on = !self.hidden.contains(&kind);
                    if ui
                        .checkbox(&mut on, format!("{} ({count})", kind.label()))
                        .changed()
                    {
                        if on {
                            self.hidden.remove(&kind);
                        } else {
                            self.hidden.insert(kind);
                        }
                        self.dirty = true;
                    }
                }
            });

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.menu_button("Exportieren", |ui| {
                    for format in Format::ALL {
                        if ui.button(format.label()).clicked() {
                            ui.close();
                            self.export(format);
                        }
                    }
                });
                if ui
                    .add_enabled(self.loading.is_none(), egui::Button::new("Neu laden"))
                    .clicked()
                {
                    self.reload(ui.ctx());
                }
                if !self.is_admin
                    && ui
                        .button("Als Administrator neu starten")
                        .on_hover_text("Nötig für Prefetch (gestartete Programme)")
                        .clicked()
                    && restart_as_admin()
                {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
        });
    }

    fn status_bar(&self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            if self.loading.is_some() {
                ui.spinner();
                ui.label("Quellen werden gelesen …");
            } else {
                ui.label(format!(
                    "{} von {} Einträgen",
                    self.visible.len(),
                    self.data.len()
                ));
            }
            for warning in &self.warnings {
                ui.separator();
                ui.label(RichText::new(warning).color(PROBLEM));
            }
            if let Some(status) = &self.status {
                ui.separator();
                ui.label(RichText::new(status).color(MUTED));
            }
            if let Some(loaded_at) = self.loaded_at {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!(
                            "Stand {} · Lokal · Portable",
                            loaded_at.format("%H:%M")
                        ))
                        .color(MUTED),
                    );
                });
            }
        });
    }

    fn table(&mut self, ui: &mut egui::Ui) {
        let mut sort_clicked = None;
        let mut row_clicked = None;
        let header = |ui: &mut egui::Ui, title: &str, key: SortKey, current: SortKey, asc: bool| {
            let response = ui.add(egui::Button::new(RichText::new(title).strong()).frame(false));
            // Sortierpfeil als Dreieck gezeichnet; die Standardschrift hat keine Pfeilzeichen.
            if key == current {
                let c = egui::pos2(response.rect.right() + 7.0, response.rect.center().y);
                let (tip, base) = if asc { (-3.5, 3.0) } else { (3.5, -3.0) };
                ui.painter().add(egui::Shape::convex_polygon(
                    vec![
                        egui::pos2(c.x - 4.0, c.y + base),
                        egui::pos2(c.x + 4.0, c.y + base),
                        egui::pos2(c.x, c.y + tip),
                    ],
                    ACCENT,
                    egui::Stroke::NONE,
                ));
            }
            response.clicked()
        };
        let (current, asc) = (self.sort, self.ascending);

        TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .sense(egui::Sense::click())
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::initial(150.0).at_least(130.0))
            .column(Column::initial(220.0).at_least(120.0).clip(true))
            .column(Column::initial(440.0).at_least(150.0).clip(true))
            .column(Column::initial(260.0).at_least(80.0).clip(true))
            .column(Column::remainder().at_least(80.0).clip(true))
            .header(26.0, |mut row| {
                for (title, key) in [
                    ("Zeit", Some(SortKey::Time)),
                    ("Aktion", Some(SortKey::Kind)),
                    ("Beschreibung", Some(SortKey::Description)),
                    ("Details", None),
                    ("Quelle", Some(SortKey::Source)),
                ] {
                    row.col(|ui| match key {
                        Some(key) => {
                            if header(ui, title, key, current, asc) {
                                sort_clicked = Some(key);
                            }
                        }
                        None => {
                            ui.strong(title);
                        }
                    });
                }
            })
            .body(|body| {
                body.rows(22.0, self.visible.len(), |mut row| {
                    let index = self.visible[row.index()];
                    let a = &self.data[index];
                    row.set_selected(self.selected == Some(index));
                    row.col(|ui| {
                        ui.label(RichText::new(a.local_time()).monospace());
                    });
                    row.col(|ui| {
                        let color = if a.kind.is_problem() { PROBLEM } else { ACCENT };
                        ui.label(RichText::new(a.kind.label()).color(color));
                    });
                    row.col(|ui| {
                        ui.label(&a.description).on_hover_text(&a.description);
                    });
                    row.col(|ui| {
                        ui.label(RichText::new(&a.details).color(MUTED));
                    });
                    row.col(|ui| {
                        ui.label(RichText::new(&a.source).color(MUTED))
                            .on_hover_text(&a.source);
                    });
                    let response = row.response();
                    if response.clicked() {
                        row_clicked = Some(index);
                    }
                    response.context_menu(|ui| row_menu(ui, a));
                });
            });

        if let Some(key) = sort_clicked {
            if self.sort == key {
                self.ascending = !self.ascending;
            } else {
                self.sort = key;
                self.ascending = key != SortKey::Time;
            }
            self.dirty = true;
        }
        if row_clicked.is_some() {
            self.selected = row_clicked;
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if let Some(collection) = self.loading.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.data = collection.activities;
            self.warnings = collection.warnings;
            self.loading = None;
            self.loaded_at = Some(Local::now());
            self.dirty = true;
        }
        if self.dirty {
            self.refilter();
            self.dirty = false;
        }

        egui::Panel::top("toolbar").show(ui, |ui| {
            ui.add_space(6.0);
            self.toolbar(ui);
            ui.add_space(6.0);
        });
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.add_space(4.0);
            self.status_bar(ui);
            ui.add_space(4.0);
        });
        egui::CentralPanel::default().show(ui, |ui| {
            if self.loading.is_some() && self.data.is_empty() {
                ui.centered_and_justified(|ui| ui.spinner());
            } else {
                self.table(ui);
            }
        });
    }
}

fn row_menu(ui: &mut egui::Ui, a: &Activity) {
    if ui.button("Zeile kopieren").clicked() {
        let line = [
            a.local_time().as_str(),
            a.kind.label(),
            &a.description,
            &a.details,
            &a.source,
        ]
        .join("\t");
        ui.ctx().copy_text(line);
        ui.close();
    }
    if ui.button("Beschreibung kopieren").clicked() {
        ui.ctx().copy_text(a.description.clone());
        ui.close();
    }
    // Nur für echte Pfade sinnvoll (z. B. "C:\…" oder "\\server\…")
    let path = &a.description;
    if (path.get(1..3) == Some(":\\") || path.starts_with("\\\\"))
        && ui.button("Im Explorer anzeigen").clicked()
    {
        let _ = std::process::Command::new("explorer")
            .arg(format!("/select,{path}"))
            .spawn();
        ui.close();
    }
}

fn restart_as_admin() -> bool {
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    use windows::core::{PCWSTR, w};
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let exe = wide(&exe.to_string_lossy());
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("runas"),
            PCWSTR(exe.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    // Werte über 32 bedeuten Erfolg; bei Abbruch der UAC-Abfrage bleibt die App offen.
    result.0 as usize > 32
}

fn apply_style(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = Color32::from_rgb(0x0E, 0x16, 0x0E);
    visuals.window_fill = Color32::from_rgb(0x12, 0x1C, 0x12);
    visuals.extreme_bg_color = Color32::from_rgb(0x08, 0x10, 0x08);
    visuals.faint_bg_color = Color32::from_rgb(0x14, 0x20, 0x14);
    visuals.selection.bg_fill = Color32::from_rgb(0x3D, 0x62, 0x38);
    visuals.selection.stroke.color = ACCENT;
    visuals.hyperlink_color = Color32::from_rgb(0x00, 0xB7, 0xFF);
    ctx.set_visuals(visuals);
}
