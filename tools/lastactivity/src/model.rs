use chrono::{DateTime, Local, Utc};

/// Art einer gefundenen Aktivität.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    ProgramRun,
    FileOpened,
    FolderOpened,
    SoftwareInstalled,
    SystemStart,
    SystemShutdown,
    UnexpectedShutdown,
    Sleep,
    Resume,
    Logon,
    Logoff,
    NetworkConnected,
    NetworkDisconnected,
    WlanConnected,
    WlanDisconnected,
    AppCrash,
    AppHang,
    BlueScreen,
    InstallerStarted,
    InstallerEnded,
    RestorePoint,
    TaskRun,
}

impl Kind {
    pub const ALL: [Kind; 22] = [
        Kind::ProgramRun,
        Kind::FileOpened,
        Kind::FolderOpened,
        Kind::SoftwareInstalled,
        Kind::SystemStart,
        Kind::SystemShutdown,
        Kind::UnexpectedShutdown,
        Kind::Sleep,
        Kind::Resume,
        Kind::Logon,
        Kind::Logoff,
        Kind::NetworkConnected,
        Kind::NetworkDisconnected,
        Kind::WlanConnected,
        Kind::WlanDisconnected,
        Kind::AppCrash,
        Kind::AppHang,
        Kind::BlueScreen,
        Kind::InstallerStarted,
        Kind::InstallerEnded,
        Kind::RestorePoint,
        Kind::TaskRun,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Kind::ProgramRun => "Programm gestartet",
            Kind::FileOpened => "Datei geöffnet",
            Kind::FolderOpened => "Ordner geöffnet",
            Kind::SoftwareInstalled => "Software installiert",
            Kind::SystemStart => "System gestartet",
            Kind::SystemShutdown => "System heruntergefahren",
            Kind::UnexpectedShutdown => "Unerwarteter Neustart",
            Kind::Sleep => "Energiesparmodus",
            Kind::Resume => "Aus Energiesparmodus aufgewacht",
            Kind::Logon => "Benutzer angemeldet",
            Kind::Logoff => "Benutzer abgemeldet",
            Kind::NetworkConnected => "Netzwerk verbunden",
            Kind::NetworkDisconnected => "Netzwerk getrennt",
            Kind::WlanConnected => "WLAN verbunden",
            Kind::WlanDisconnected => "WLAN getrennt",
            Kind::AppCrash => "Programm abgestürzt",
            Kind::AppHang => "Programm reagiert nicht",
            Kind::BlueScreen => "Bluescreen",
            Kind::InstallerStarted => "Windows Installer gestartet",
            Kind::InstallerEnded => "Windows Installer beendet",
            Kind::RestorePoint => "Wiederherstellungspunkt erstellt",
            Kind::TaskRun => "Geplante Aufgabe ausgeführt",
        }
    }

    /// Hervorgehobene Ereignisse (Fehler) bekommen in der Oberfläche eine Warnfarbe.
    pub fn is_problem(self) -> bool {
        matches!(
            self,
            Kind::AppCrash | Kind::AppHang | Kind::BlueScreen | Kind::UnexpectedShutdown
        )
    }
}

#[derive(Clone, Debug)]
pub struct Activity {
    pub time: DateTime<Utc>,
    pub kind: Kind,
    pub description: String,
    pub details: String,
    pub source: String,
    pub extension: String,
}

impl Activity {
    pub fn local_time(&self) -> String {
        self.time
            .with_timezone(&Local)
            .format("%d.%m.%Y %H:%M:%S")
            .to_string()
    }
}
