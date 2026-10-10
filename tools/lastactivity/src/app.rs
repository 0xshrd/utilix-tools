use crate::export::{self, Format};
use crate::model::{Activity, Kind};
use crate::sources::{self, Collection};
use crate::util::wide;
use chrono::{DateTime, Duration, Local, Utc};
use eframe::egui::{
    self, Align, Align2, Color32, CursorIcon, FontFamily, FontId, Id, Layout, Margin,
    PointerButton, PopupCloseBehavior, Rect, ResizeDirection, RichText, Sense, Stroke, StrokeKind,
    UiBuilder, Vec2, ViewportCommand, pos2, vec2,
};
use egui_phosphor::regular as icon;
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};

/// Farben der utilix-Tools. Eine durchgehende grüne Fläche, Abstufung nur über Linien.
mod theme {
    use eframe::egui::Color32;

    pub const BG: Color32 = Color32::from_rgb(0x12, 0x2A, 0x1C);
    pub const LINE: Color32 = Color32::from_rgb(0x22, 0x40, 0x2D);
    pub const LINE_STRONG: Color32 = Color32::from_rgb(0x34, 0x5C, 0x40);
    pub const HOVER: Color32 = Color32::from_rgb(0x18, 0x35, 0x24);
    pub const SELECTED: Color32 = Color32::from_rgb(0x1F, 0x41, 0x2C);
    pub const ACCENT: Color32 = Color32::from_rgb(0xBF, 0xCF, 0x5B);
    pub const ON_ACCENT: Color32 = Color32::from_rgb(0x0E, 0x33, 0x0D);
    pub const TEXT: Color32 = Color32::from_rgb(0xE6, 0xEE, 0xE4);
    pub const MUTED: Color32 = Color32::from_rgb(0x9C, 0xB0, 0xA1);
    pub const DIM: Color32 = Color32::from_rgb(0x6A, 0x84, 0x71);
    pub const WARN: Color32 = Color32::from_rgb(0xF2, 0xA6, 0x4B);
    pub const DANGER: Color32 = Color32::from_rgb(0xC4, 0x3B, 0x2E);
}
use theme::*;

const ROW_HEIGHT: f32 = 44.0;
const CELL_PAD: f32 = 20.0;

pub fn run() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("LastActivity · utilix")
            .with_decorations(false)
            .with_inner_size([1360.0, 860.0])
            .with_min_inner_size([760.0, 420.0])
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

fn sans(size: f32) -> FontId {
    FontId::proportional(size)
}
fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("semibold".into()))
}
fn mono(size: f32) -> FontId {
    FontId::monospace(size)
}
fn mono_medium(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("mono-medium".into()))
}
fn icon_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("phosphor".into()))
}

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
            Group::All => "Alle",
            Group::Programs => "Programme",
            Group::Files => "Dateien",
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
    Description,
    Kind,
    Time,
    Details,
    Source,
}

/// Spalten mit Anteil an der Tabellenbreite.
const COLUMNS: [(&str, SortKey, f32); 5] = [
    ("Beschreibung", SortKey::Description, 0.35),
    ("Aktion", SortKey::Kind, 0.18),
    ("Zeit", SortKey::Time, 0.13),
    ("Details", SortKey::Details, 0.18),
    ("Quelle", SortKey::Source, 0.16),
];

fn kind_icon(kind: Kind) -> &'static str {
    use Kind::*;
    match kind {
        ProgramRun => icon::APP_WINDOW,
        FileOpened => icon::FILE,
        FolderOpened => icon::FOLDER_OPEN,
        FolderViewed => icon::FOLDER,
        FileSelected => icon::FILE_ARROW_DOWN,
        SoftwareInstalled => icon::PACKAGE,
        SystemStart | SystemShutdown => icon::POWER,
        UnexpectedShutdown => icon::LIGHTNING,
        Sleep => icon::MOON,
        Resume => icon::SUN,
        Logon => icon::SIGN_IN,
        Logoff => icon::SIGN_OUT,
        NetworkConnected => icon::PLUGS_CONNECTED,
        NetworkDisconnected => icon::PLUGS,
        WlanConnected => icon::WIFI_HIGH,
        WlanDisconnected => icon::WIFI_SLASH,
        AppCrash => icon::WARNING,
        AppHang => icon::HOURGLASS,
        BlueScreen => icon::WARNING_OCTAGON,
        InstallerStarted | InstallerEnded => icon::DOWNLOAD_SIMPLE,
        RestorePoint => icon::CLOCK_COUNTER_CLOCKWISE,
        TaskRun => icon::CALENDAR_CHECK,
    }
}

/// Punktfarbe: Fehler orange, "endende" Ereignisse grau, alles andere im Akzent.
fn kind_color(kind: Kind) -> Color32 {
    use Kind::*;
    match kind {
        _ if kind.is_problem() => WARN,
        SystemShutdown | Sleep | Logoff | NetworkDisconnected | WlanDisconnected
        | InstallerEnded => DIM,
        _ => ACCENT,
    }
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
    corners_rounded: bool,
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
            corners_rounded: false,
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
                SortKey::Details => a.details.cmp(&b.details),
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

    // ---------------------------------------------------------------- Titelleiste

    fn title_bar(&mut self, ui: &mut egui::Ui) {
        let rect = ui.max_rect();
        let maximized = ui.input(|i| i.viewport().maximized.unwrap_or(false));

        // Hintergrund zuerst registrieren, damit die Knöpfe darüber Vorrang haben.
        let drag = ui.interact(rect, Id::new("titlebar"), Sense::click_and_drag());
        if drag.double_clicked() {
            ui.ctx()
                .send_viewport_cmd(ViewportCommand::Maximized(!maximized));
        } else if drag.drag_started_by(PointerButton::Primary) {
            ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
        }

        ui.horizontal_centered(|ui| {
            ui.add_space(16.0);
            let (badge, _) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::hover());
            ui.painter().rect_filled(badge, 6.0, ACCENT);
            ui.painter().text(
                badge.center(),
                Align2::CENTER_CENTER,
                icon::CLOCK_COUNTER_CLOCKWISE,
                icon_font(15.0),
                ON_ACCENT,
            );
            ui.add_space(10.0);
            ui.label(RichText::new("utilix").font(mono(12.0)).color(ACCENT));
            ui.add_space(4.0);
            ui.label(
                RichText::new("LastActivity")
                    .font(semibold(14.5))
                    .color(TEXT),
            );

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                if window_button(ui, icon::X, true).clicked() {
                    ui.ctx().send_viewport_cmd(ViewportCommand::Close);
                }
                let max_icon = if maximized {
                    icon::CORNERS_IN
                } else {
                    icon::SQUARE
                };
                if window_button(ui, max_icon, false).clicked() {
                    ui.ctx()
                        .send_viewport_cmd(ViewportCommand::Maximized(!maximized));
                }
                if window_button(ui, icon::MINUS, false).clicked() {
                    ui.ctx().send_viewport_cmd(ViewportCommand::Minimized(true));
                }

                ui.add_space(8.0);
                let (sep, _) = ui.allocate_exact_size(vec2(1.0, 22.0), Sense::hover());
                ui.painter()
                    .vline(sep.center().x, sep.y_range(), Stroke::new(1.0, LINE_STRONG));
                ui.add_space(8.0);

                let export = icon_button(ui, icon::EXPORT, "Exportieren", MUTED);
                egui::Popup::menu(&export).show(|ui| {
                    ui.set_min_width(180.0);
                    for format in Format::ALL {
                        if ui.button(format.label()).clicked() {
                            ui.close();
                            self.export(format);
                        }
                    }
                });

                let (shield, color, tip) = if self.is_admin {
                    (icon::SHIELD_CHECK, ACCENT, "Läuft als Administrator")
                } else {
                    (
                        icon::SHIELD_WARNING,
                        WARN,
                        "Als Administrator neu starten (nötig für Programmstarts aus Prefetch)",
                    )
                };
                if icon_button(ui, shield, tip, color).clicked()
                    && !self.is_admin
                    && restart_as_admin()
                {
                    ui.ctx().send_viewport_cmd(ViewportCommand::Close);
                }

                let reload = icon_button(ui, icon::ARROWS_CLOCKWISE, "Neu laden", MUTED);
                if reload.clicked() && self.loading.is_none() {
                    self.reload(ui.ctx());
                }
            });
        });
    }

    // ---------------------------------------------------------------- Werkzeugleiste

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_centered(|ui| {
            self.segmented(ui);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let filtered = self.range != Range::All || !self.hidden.is_empty();
                let filter = icon_button(
                    ui,
                    icon::SLIDERS_HORIZONTAL,
                    "Zeitraum und Aktionen",
                    if filtered { ACCENT } else { MUTED },
                );
                egui::Popup::from_toggle_button_response(&filter)
                    .close_behavior(PopupCloseBehavior::CloseOnClickOutside)
                    .show(|ui| self.filter_menu(ui));
                ui.add_space(10.0);
                self.search(ui);
            });
        });
    }

    fn segmented(&mut self, ui: &mut egui::Ui) {
        let font = mono_medium(13.0);
        let widths: Vec<f32> = Group::ALL
            .iter()
            .map(|g| {
                let galley = ui
                    .painter()
                    .layout_no_wrap(g.label().into(), font.clone(), TEXT);
                galley.size().x + 32.0
            })
            .collect();
        let (outer, _) =
            ui.allocate_exact_size(vec2(widths.iter().sum::<f32>() + 8.0, 38.0), Sense::hover());
        ui.painter().rect_stroke(
            outer,
            10.0,
            Stroke::new(1.0, LINE_STRONG),
            StrokeKind::Inside,
        );

        let mut x = outer.left() + 4.0;
        for (group, width) in Group::ALL.into_iter().zip(widths) {
            let rect = Rect::from_min_size(pos2(x, outer.top() + 4.0), vec2(width, 30.0));
            let response = ui.interact(rect, Id::new(("group", group.label())), Sense::click());
            let selected = self.group == group;
            let color = if selected {
                ui.painter().rect_filled(rect, 7.0, ACCENT);
                ON_ACCENT
            } else if response.hovered() {
                ui.painter().rect_filled(rect, 7.0, HOVER);
                TEXT
            } else {
                MUTED
            };
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                group.label(),
                font.clone(),
                color,
            );
            if response.clicked() {
                self.group = group;
                self.dirty = true;
            }
            x += width;
        }
    }

    fn search(&mut self, ui: &mut egui::Ui) {
        let (rect, _) = ui.allocate_exact_size(vec2(320.0, 38.0), Sense::hover());
        let search_id = Id::new("search");
        let focused = ui.memory(|m| m.has_focus(search_id));
        ui.painter().rect_stroke(
            rect,
            10.0,
            Stroke::new(1.0, if focused { ACCENT } else { LINE_STRONG }),
            StrokeKind::Inside,
        );
        ui.painter().text(
            pos2(rect.left() + 16.0, rect.center().y),
            Align2::LEFT_CENTER,
            icon::MAGNIFYING_GLASS,
            icon_font(16.0),
            MUTED,
        );

        let kbd = ui
            .painter()
            .layout_no_wrap("Strg F".into(), mono(11.0), MUTED);
        let kbd_rect = Rect::from_center_size(
            pos2(
                rect.right() - 14.0 - (kbd.size().x + 12.0) / 2.0,
                rect.center().y,
            ),
            vec2(kbd.size().x + 12.0, 20.0),
        );
        ui.painter().rect_stroke(
            kbd_rect,
            4.0,
            Stroke::new(1.0, LINE_STRONG),
            StrokeKind::Inside,
        );
        ui.painter()
            .galley(kbd_rect.center() - kbd.size() / 2.0, kbd, MUTED);

        let edit_rect = Rect::from_min_max(
            pos2(rect.left() + 40.0, rect.top()),
            pos2(kbd_rect.left() - 8.0, rect.bottom()),
        );
        let mut child = ui.new_child(
            UiBuilder::new()
                .max_rect(edit_rect)
                .layout(Layout::left_to_right(Align::Center)),
        );
        let response = child.add(
            egui::TextEdit::singleline(&mut self.query)
                .id(search_id)
                .frame(egui::Frame::NONE)
                .margin(Margin::ZERO)
                .font(sans(14.0))
                .text_color(TEXT)
                .hint_text(RichText::new("Aktivitäten durchsuchen").color(DIM))
                .desired_width(edit_rect.width()),
        );
        if response.changed() {
            self.dirty = true;
        }
        if ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::F)) {
            response.request_focus();
        }
        if response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.query.clear();
            self.dirty = true;
        }
    }

    fn filter_menu(&mut self, ui: &mut egui::Ui) {
        ui.set_min_width(280.0);
        ui.label(RichText::new("ZEITRAUM").font(mono_medium(11.0)).color(DIM));
        for range in Range::ALL {
            if ui
                .radio_value(&mut self.range, range, range.label())
                .changed()
            {
                self.dirty = true;
            }
        }
        ui.add_space(6.0);
        ui.separator();
        ui.horizontal(|ui| {
            ui.label(RichText::new("AKTIONEN").font(mono_medium(11.0)).color(DIM));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.small_button("Keine").clicked() {
                    self.hidden.extend(Kind::ALL);
                    self.dirty = true;
                }
                if ui.small_button("Alle").clicked() {
                    self.hidden.clear();
                    self.dirty = true;
                }
            });
        });
        egui::ScrollArea::vertical()
            .max_height(380.0)
            .show(ui, |ui| {
                for kind in Kind::ALL {
                    let count = self.data.iter().filter(|a| a.kind == kind).count();
                    let mut on = !self.hidden.contains(&kind);
                    let label = format!("{}  {} ({count})", kind_icon(kind), kind.label());
                    if ui.checkbox(&mut on, label).changed() {
                        if on {
                            self.hidden.remove(&kind);
                        } else {
                            self.hidden.insert(kind);
                        }
                        self.dirty = true;
                    }
                }
            });
    }

    // ---------------------------------------------------------------- Tabelle

    fn table(&mut self, ui: &mut egui::Ui) {
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        let width = ui.available_width();
        let columns: Vec<(f32, f32)> = {
            let mut x = ui.max_rect().left();
            COLUMNS
                .iter()
                .map(|&(_, _, share)| {
                    let w = width * share;
                    let col = (x, w);
                    x += w;
                    col
                })
                .collect()
        };

        // Kopfzeile
        let (header, _) = ui.allocate_exact_size(vec2(width, 48.0), Sense::hover());
        ui.painter().hline(
            header.x_range(),
            header.bottom() - 0.5,
            Stroke::new(1.0, LINE_STRONG),
        );
        for (&(title, key, _), &(x, w)) in COLUMNS.iter().zip(&columns) {
            let cell = Rect::from_min_size(pos2(x, header.top()), vec2(w, header.height()));
            let response = ui
                .interact(cell, Id::new(("sort", title)), Sense::click())
                .on_hover_cursor(CursorIcon::PointingHand);
            let text_color = if response.hovered() { ACCENT } else { TEXT };
            let text_rect = ui.painter().text(
                pos2(cell.left() + CELL_PAD, cell.center().y),
                Align2::LEFT_CENTER,
                title,
                mono_medium(12.0),
                text_color,
            );
            if self.sort == key {
                let arrow = if self.ascending {
                    icon::ARROW_UP
                } else {
                    icon::ARROW_DOWN
                };
                ui.painter().text(
                    pos2(text_rect.right() + 6.0, cell.center().y),
                    Align2::LEFT_CENTER,
                    arrow,
                    icon_font(13.0),
                    ACCENT,
                );
            }
            if response.clicked() {
                if self.sort == key {
                    self.ascending = !self.ascending;
                } else {
                    self.sort = key;
                    self.ascending = key != SortKey::Time;
                }
                self.dirty = true;
            }
        }

        if self.visible.is_empty() {
            let message = if self.loading.is_some() {
                "Quellen werden gelesen …"
            } else {
                "Keine Einträge für diese Filter"
            };
            ui.centered_and_justified(|ui| {
                ui.label(RichText::new(message).font(mono(13.0)).color(DIM));
            });
            return;
        }

        let mut clicked = None;
        egui::ScrollArea::vertical().auto_shrink(false).show_rows(
            ui,
            ROW_HEIGHT,
            self.visible.len(),
            |ui, rows| {
                for row in rows {
                    let index = self.visible[row];
                    let response = draw_row(
                        ui,
                        &self.data[index],
                        self.selected == Some(index),
                        &columns,
                    );
                    if response.clicked() {
                        clicked = Some(index);
                    }
                }
            },
        );
        if clicked.is_some() {
            self.selected = clicked;
        }
    }

    // ---------------------------------------------------------------- Statuszeile

    fn status_bar(&self, ui: &mut egui::Ui) {
        ui.horizontal_centered(|ui| {
            ui.add_space(20.0);
            let small = mono(11.0);
            if self.loading.is_some() {
                ui.add(egui::Spinner::new().size(12.0).color(ACCENT));
                ui.add_space(8.0);
                ui.label(
                    RichText::new("Quellen werden gelesen …")
                        .font(small.clone())
                        .color(MUTED),
                );
            } else {
                let mut text = format!("{} Einträge", self.visible.len());
                if self.visible.len() != self.data.len() {
                    text.push_str(&format!(" von {}", self.data.len()));
                }
                ui.label(RichText::new(text).font(small.clone()).color(MUTED));
            }
            for warning in &self.warnings {
                ui.label(
                    RichText::new(format!("  ·  {warning}"))
                        .font(small.clone())
                        .color(WARN),
                );
            }
            if let Some(status) = &self.status {
                ui.label(
                    RichText::new(format!("  ·  {status}"))
                        .font(small.clone())
                        .color(MUTED),
                );
            }

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(20.0);
                let stamp = self
                    .loaded_at
                    .map(|t| t.format("%H:%M").to_string())
                    .unwrap_or_else(|| "–".into());
                ui.label(
                    RichText::new(format!("Stand {stamp} · Lokal · Portable"))
                        .font(small.clone())
                        .color(MUTED),
                );
                ui.add_space(6.0);
                let (dot, _) = ui.allocate_exact_size(vec2(6.0, 6.0), Sense::hover());
                ui.painter().circle_filled(dot.center(), 3.0, ACCENT);
            });
        });
    }

    // ---------------------------------------------------------------- Fenster

    /// Ränder zum Vergrößern, da das Fenster keinen Windows-Rahmen hat.
    fn resize_handles(&self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().maximized.unwrap_or(false)) {
            return;
        }
        let Some(pos) = ctx.input(|i| i.pointer.hover_pos()) else {
            return;
        };
        let r = ctx.viewport_rect();
        const EDGE: f32 = 6.0;
        let (left, right) = (pos.x <= r.left() + EDGE, pos.x >= r.right() - EDGE);
        let (top, bottom) = (pos.y <= r.top() + EDGE, pos.y >= r.bottom() - EDGE);
        let direction = match (left, right, top, bottom) {
            (true, _, true, _) => ResizeDirection::NorthWest,
            (_, true, true, _) => ResizeDirection::NorthEast,
            (true, _, _, true) => ResizeDirection::SouthWest,
            (_, true, _, true) => ResizeDirection::SouthEast,
            (true, ..) => ResizeDirection::West,
            (_, true, ..) => ResizeDirection::East,
            (_, _, true, _) => ResizeDirection::North,
            (_, _, _, true) => ResizeDirection::South,
            _ => return,
        };
        ctx.set_cursor_icon(match direction {
            ResizeDirection::North | ResizeDirection::South => CursorIcon::ResizeVertical,
            ResizeDirection::East | ResizeDirection::West => CursorIcon::ResizeHorizontal,
            ResizeDirection::NorthWest | ResizeDirection::SouthEast => CursorIcon::ResizeNwSe,
            ResizeDirection::NorthEast | ResizeDirection::SouthWest => CursorIcon::ResizeNeSw,
        });
        if ctx.input(|i| i.pointer.primary_pressed()) {
            ctx.send_viewport_cmd(ViewportCommand::BeginResize(direction));
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        if !self.corners_rounded {
            round_corners(frame);
            self.corners_rounded = true;
        }
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
        let ctx = ui.ctx().clone();
        self.resize_handles(&ctx);

        let bar = |margin: Margin| egui::Frame::new().fill(BG).inner_margin(margin);
        egui::Panel::top("titlebar")
            .exact_size(44.0)
            .frame(bar(Margin::ZERO))
            .show(ui, |ui| self.title_bar(ui));
        egui::Panel::top("toolbar")
            .exact_size(72.0)
            .frame(bar(Margin::symmetric(24, 0)))
            .show(ui, |ui| self.toolbar(ui));
        egui::Panel::bottom("statusbar")
            .exact_size(30.0)
            .frame(bar(Margin::ZERO))
            .show(ui, |ui| self.status_bar(ui));
        egui::CentralPanel::default()
            .frame(bar(Margin::ZERO))
            .show(ui, |ui| self.table(ui));
    }
}

fn draw_row(
    ui: &mut egui::Ui,
    a: &Activity,
    selected: bool,
    columns: &[(f32, f32)],
) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), ROW_HEIGHT), Sense::click());
    let painter = ui.painter();
    if selected {
        painter.rect_filled(rect, 0.0, SELECTED);
    } else if response.hovered() {
        painter.rect_filled(rect, 0.0, HOVER);
    }
    painter.hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1.0, LINE));

    let cell = |i: usize| {
        let (x, w) = columns[i];
        Rect::from_min_max(
            pos2(x + CELL_PAD, rect.top()),
            pos2(x + w - 8.0, rect.bottom()),
        )
    };

    // Beschreibung mit Icon-Kachel
    let c0 = cell(0);
    let tile = Rect::from_min_size(pos2(c0.left(), rect.center().y - 13.0), vec2(26.0, 26.0));
    painter.rect_stroke(tile, 6.0, Stroke::new(1.0, LINE_STRONG), StrokeKind::Inside);
    painter.text(
        tile.center(),
        Align2::CENTER_CENTER,
        kind_icon(a.kind),
        icon_font(14.0),
        MUTED,
    );
    let description = Rect::from_min_max(pos2(tile.right() + 12.0, c0.top()), c0.max);
    cell_label(ui, description, &a.description, sans(14.0), TEXT);

    // Aktion mit Statuspunkt
    let c1 = cell(1);
    let color = kind_color(a.kind);
    ui.painter()
        .circle_filled(pos2(c1.left() + 3.0, rect.center().y), 3.0, color);
    let label_rect = Rect::from_min_max(pos2(c1.left() + 12.0, c1.top()), c1.max);
    cell_label(
        ui,
        label_rect,
        a.kind.label(),
        mono(12.0),
        if color == DIM { MUTED } else { TEXT },
    );

    cell_label(ui, cell(2), &a.local_time(), mono(12.0), MUTED);
    cell_label(ui, cell(3), &a.details, mono(12.0), MUTED);
    cell_label(ui, cell(4), &a.source, mono(12.0), DIM);

    response.context_menu(|ui| row_menu(ui, a));
    response
}

/// Einzeilige Zelle, bei Überlänge mit "…" gekürzt (voller Text im Tooltip). Leer wird zu "—".
fn cell_label(ui: &mut egui::Ui, rect: Rect, text: &str, font: FontId, color: Color32) {
    let (text, color) = if text.is_empty() {
        ("—", DIM)
    } else {
        (text, color)
    };
    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::left_to_right(Align::Center)),
    );
    child.add(egui::Label::new(RichText::new(text).font(font).color(color)).truncate());
}

fn icon_button(ui: &mut egui::Ui, glyph: &str, tooltip: &str, color: Color32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(36.0, 32.0), Sense::click());
    let hovered = response.hovered();
    if hovered {
        ui.painter().rect_filled(rect, 7.0, HOVER);
    }
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        glyph,
        icon_font(17.0),
        if hovered { TEXT } else { color },
    );
    response.on_hover_text(tooltip)
}

fn window_button(ui: &mut egui::Ui, glyph: &str, close: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(46.0, 44.0), Sense::click());
    let hovered = response.hovered();
    if hovered {
        ui.painter()
            .rect_filled(rect, 0.0, if close { DANGER } else { HOVER });
    }
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        glyph,
        icon_font(15.0),
        if hovered { TEXT } else { MUTED },
    );
    response
}

fn row_menu(ui: &mut egui::Ui, a: &Activity) {
    if ui
        .button(format!("{}  Zeile kopieren", icon::COPY))
        .clicked()
    {
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
    if ui
        .button(format!("{}  Beschreibung kopieren", icon::COPY))
        .clicked()
    {
        ui.ctx().copy_text(a.description.clone());
        ui.close();
    }
    // Nur für echte Pfade sinnvoll (z. B. "C:\…" oder "\\server\…")
    let path = &a.description;
    if (path.get(1..3) == Some(":\\") || path.starts_with("\\\\"))
        && ui
            .button(format!("{}  Im Explorer anzeigen", icon::FOLDER_OPEN))
            .clicked()
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

/// Abgerundete Ecken wie bei normalen Windows-11-Fenstern (ohne Wirkung auf Windows 10).
fn round_corners(frame: &eframe::Frame) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Dwm::{
        DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmSetWindowAttribute,
    };
    let Ok(handle) = frame.window_handle() else {
        return;
    };
    if let RawWindowHandle::Win32(win) = handle.as_raw() {
        let hwnd = HWND(win.hwnd.get() as *mut _);
        let preference = DWMWCP_ROUND;
        unsafe {
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_WINDOW_CORNER_PREFERENCE,
                (&raw const preference).cast(),
                size_of_val(&preference) as u32,
            );
        }
    }
}

fn apply_style(ctx: &egui::Context) {
    install_fonts(ctx);
    ctx.set_theme(egui::Theme::Dark);
    ctx.all_styles_mut(|style| {
        style.interaction.selectable_labels = false;
        style.spacing.button_padding = vec2(10.0, 5.0);
        style.spacing.menu_margin = Margin::same(8);

        let v = &mut style.visuals;
        *v = egui::Visuals::dark();
        v.panel_fill = BG;
        v.window_fill = BG;
        v.extreme_bg_color = BG;
        v.faint_bg_color = HOVER;
        v.window_stroke = Stroke::new(1.0, LINE_STRONG);
        v.window_corner_radius = 10.into();
        v.menu_corner_radius = 10.into();
        v.selection.bg_fill = Color32::from_rgba_unmultiplied(0xBF, 0xCF, 0x5B, 70);
        v.selection.stroke = Stroke::new(1.0, ACCENT);
        v.hyperlink_color = ACCENT;
        v.override_text_color = Some(TEXT);

        let w = &mut v.widgets;
        w.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
        w.noninteractive.fg_stroke = Stroke::new(1.0, MUTED);
        for state in [&mut w.inactive, &mut w.hovered, &mut w.active, &mut w.open] {
            state.corner_radius = 6.into();
        }
        w.inactive.weak_bg_fill = Color32::TRANSPARENT;
        w.inactive.bg_fill = LINE;
        w.inactive.bg_stroke = Stroke::new(1.0, LINE_STRONG);
        w.hovered.weak_bg_fill = HOVER;
        w.hovered.bg_fill = HOVER;
        w.hovered.bg_stroke = Stroke::new(1.0, ACCENT);
        w.active.weak_bg_fill = SELECTED;
        w.active.bg_fill = SELECTED;
        w.open.weak_bg_fill = HOVER;
    });
}

fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let mut add = |name: &str, bytes: &'static [u8]| {
        fonts
            .font_data
            .insert(name.into(), Arc::new(egui::FontData::from_static(bytes)));
    };
    add(
        "geist",
        include_bytes!("../../../assets/fonts/Geist-Regular.ttf"),
    );
    add(
        "geist-semibold",
        include_bytes!("../../../assets/fonts/Geist-SemiBold.ttf"),
    );
    add(
        "geist-mono",
        include_bytes!("../../../assets/fonts/GeistMono-Regular.ttf"),
    );
    add(
        "geist-mono-medium",
        include_bytes!("../../../assets/fonts/GeistMono-Medium.ttf"),
    );

    let families = &mut fonts.families;
    families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "geist".into());
    families
        .entry(FontFamily::Monospace)
        .or_default()
        .insert(0, "geist-mono".into());
    families.insert(
        FontFamily::Name("semibold".into()),
        vec!["geist-semibold".into(), "geist".into()],
    );
    families.insert(
        FontFamily::Name("mono-medium".into()),
        vec!["geist-mono-medium".into(), "geist-mono".into()],
    );
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    ctx.set_fonts(fonts);
}
