//! Ereignisprotokolle: Start/Herunterfahren, Energiesparen, Anmeldung, Abstürze, Netzwerk usw.

use super::Collection;
use crate::model::{Activity, Kind};
use crate::util::{extension_of, from_wide, wide};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use windows::Win32::Foundation::{HLOCAL, LocalFree};
use windows::Win32::Security::Authorization::ConvertStringSidToSidW;
use windows::Win32::Security::{LookupAccountSidW, PSID, SID_NAME_USE};
use windows::Win32::System::EventLog::{
    EVT_HANDLE, EvtClose, EvtNext, EvtQuery, EvtQueryChannelPath, EvtQueryReverseDirection,
    EvtRender, EvtRenderEventXml,
};
use windows::core::{PCWSTR, PWSTR};

/// Obergrenze pro Abfrage, damit sehr volle Protokolle die Oberfläche nicht blockieren.
const LIMIT_PER_QUERY: usize = 20_000;

struct RawEvent {
    provider: String,
    id: u32,
    time: DateTime<Utc>,
    data: Vec<(String, String)>,
}

impl RawEvent {
    /// Wert eines benannten Felds, ersatzweise das Feld an Position `index`
    /// (klassische Ereignisse liefern ihre Daten ohne Namen).
    fn get(&self, name: &str, index: usize) -> &str {
        self.data
            .iter()
            .find(|(k, _)| !name.is_empty() && k.eq_ignore_ascii_case(name))
            .or_else(|| {
                self.data
                    .get(index)
                    .filter(|(k, _)| k.parse::<usize>().is_ok())
            })
            .map(|(_, v)| v.as_str())
            .unwrap_or("")
    }
}

struct Query {
    channel: &'static str,
    xpath: &'static str,
    /// Optionale Protokolle fehlen auf manchen Systemen oder sind abgeschaltet; kein Hinweis nötig.
    optional: bool,
}

const QUERIES: &[Query] = &[
    Query {
        channel: "System",
        xpath: "*[System[(Provider[@Name='Microsoft-Windows-Kernel-General'] and (EventID=12 or EventID=13)) \
                or (Provider[@Name='Microsoft-Windows-Kernel-Power'] and (EventID=41 or EventID=42)) \
                or (Provider[@Name='Microsoft-Windows-Power-Troubleshooter'] and EventID=1) \
                or (Provider[@Name='Microsoft-Windows-Winlogon'] and (EventID=7001 or EventID=7002)) \
                or (Provider[@Name='Microsoft-Windows-WER-SystemErrorReporting'] and EventID=1001) \
                or (Provider[@Name='BugCheck'] and EventID=1001)]]",
        optional: false,
    },
    Query {
        channel: "Application",
        xpath: "*[System[(Provider[@Name='Application Error'] and EventID=1000) \
                or (Provider[@Name='Application Hang'] and EventID=1002) \
                or (Provider[@Name='MsiInstaller'] and (EventID=1040 or EventID=1042)) \
                or (Provider[@Name='SystemRestore'] and EventID=8194)]]",
        optional: false,
    },
    Query {
        channel: "Microsoft-Windows-NetworkProfile/Operational",
        xpath: "*[System[(EventID=10000 or EventID=10001)]]",
        optional: true,
    },
    Query {
        channel: "Microsoft-Windows-WLAN-AutoConfig/Operational",
        xpath: "*[System[(EventID=8001 or EventID=8003)]]",
        optional: true,
    },
    Query {
        channel: "Microsoft-Windows-TaskScheduler/Operational",
        xpath: "*[System[(EventID=200)]]",
        optional: true,
    },
];

pub fn collect(out: &mut Collection) {
    let mut accounts = HashMap::new();
    for q in QUERIES {
        match query(q.channel, q.xpath) {
            Ok(events) => {
                out.activities.extend(
                    events
                        .iter()
                        .filter_map(|ev| convert(q.channel, ev, &mut accounts)),
                );
            }
            Err(e) if !q.optional => {
                out.warnings
                    .push(format!("Ereignisprotokoll {}: {e}", q.channel));
            }
            Err(_) => {}
        }
    }
}

fn convert(
    channel: &str,
    ev: &RawEvent,
    accounts: &mut HashMap<String, String>,
) -> Option<Activity> {
    let (kind, description, details) = match (ev.provider.as_str(), ev.id) {
        ("Microsoft-Windows-Kernel-General", 12) => (
            Kind::SystemStart,
            "Windows gestartet".to_string(),
            String::new(),
        ),
        ("Microsoft-Windows-Kernel-General", 13) => (
            Kind::SystemShutdown,
            "Windows heruntergefahren".to_string(),
            String::new(),
        ),
        ("Microsoft-Windows-Kernel-Power", 41) => (
            Kind::UnexpectedShutdown,
            "Neustart ohne sauberes Herunterfahren".to_string(),
            bugcheck_details(ev.get("BugcheckCode", 0)),
        ),
        ("Microsoft-Windows-Kernel-Power", 42) => (
            Kind::Sleep,
            sleep_state(ev.get("TargetState", 0)).to_string(),
            String::new(),
        ),
        ("Microsoft-Windows-Power-Troubleshooter", 1) => {
            let source = ev.get("WakeSourceText", 7);
            let details = if source.is_empty() {
                String::new()
            } else {
                format!("Weckquelle: {source}")
            };
            (
                Kind::Resume,
                "Aus dem Energiesparmodus aufgewacht".to_string(),
                details,
            )
        }
        ("Microsoft-Windows-Winlogon", id @ (7001 | 7002)) => {
            let user = account_name(ev.get("UserSid", 1), accounts);
            let kind = if id == 7001 {
                Kind::Logon
            } else {
                Kind::Logoff
            };
            (kind, user, String::new())
        }
        ("Microsoft-Windows-WER-SystemErrorReporting" | "BugCheck", 1001) => (
            Kind::BlueScreen,
            format!("Bugcheck {}", ev.get("param1", 0)),
            ev.get("param2", 1).to_string(),
        ),
        ("Application Error", 1000) => (
            Kind::AppCrash,
            first_non_empty(&[ev.get("AppPath", 10), ev.get("AppName", 0)]),
            format!(
                "Modul: {} · Fehlercode: {}",
                ev.get("ModuleName", 3),
                ev.get("ExceptionCode", 6)
            ),
        ),
        ("Application Hang", 1002) => (
            Kind::AppHang,
            first_non_empty(&[ev.get("AppPath", 5), ev.get("AppName", 0)]),
            format!("Version {}", ev.get("AppVersion", 1)),
        ),
        ("MsiInstaller", 1040) => (
            Kind::InstallerStarted,
            ev.get("", 0).to_string(),
            String::new(),
        ),
        ("MsiInstaller", 1042) => (
            Kind::InstallerEnded,
            ev.get("", 0).to_string(),
            String::new(),
        ),
        ("SystemRestore", 8194) => (
            Kind::RestorePoint,
            ev.get("Description", 0).to_string(),
            String::new(),
        ),
        (_, id @ (10000 | 10001)) if channel.contains("NetworkProfile") => {
            let kind = if id == 10000 {
                Kind::NetworkConnected
            } else {
                Kind::NetworkDisconnected
            };
            (
                kind,
                ev.get("Name", 0).to_string(),
                ev.get("Description", 1).to_string(),
            )
        }
        (_, 8001) if channel.contains("WLAN") => (
            Kind::WlanConnected,
            ev.get("SSID", 3).to_string(),
            format!(
                "{} · {} / {}",
                ev.get("InterfaceDescription", 1),
                ev.get("AuthenticationAlgorithm", 7),
                ev.get("CipherAlgorithm", 8)
            ),
        ),
        (_, 8003) if channel.contains("WLAN") => (
            Kind::WlanDisconnected,
            ev.get("SSID", 3).to_string(),
            ev.get("Reason", 6).to_string(),
        ),
        (_, 200) if channel.contains("TaskScheduler") => (
            Kind::TaskRun,
            ev.get("TaskName", 0).to_string(),
            ev.get("ActionName", 1).to_string(),
        ),
        _ => return None,
    };
    Some(Activity {
        time: ev.time,
        kind,
        extension: if matches!(
            kind,
            Kind::AppCrash | Kind::AppHang | Kind::InstallerStarted | Kind::InstallerEnded
        ) {
            extension_of(&description)
        } else {
            String::new()
        },
        description,
        details,
        source: format!("Ereignisprotokoll {channel} (ID {})", ev.id),
    })
}

fn sleep_state(state: &str) -> &'static str {
    match state {
        "4" => "Energiesparmodus (S3)",
        "5" | "6" => "Ruhezustand (S4)",
        _ => "Energiesparmodus",
    }
}

fn bugcheck_details(code: &str) -> String {
    match code.parse::<u64>() {
        Ok(0) | Err(_) => String::new(),
        Ok(c) => format!("Bugcheck-Code 0x{c:08X}"),
    }
}

fn first_non_empty(values: &[&str]) -> String {
    values
        .iter()
        .find(|v| !v.is_empty())
        .copied()
        .unwrap_or("")
        .to_string()
}

fn query(channel: &str, xpath: &str) -> Result<Vec<RawEvent>, String> {
    let channel_w = wide(channel);
    let xpath_w = wide(xpath);
    let flags = EvtQueryChannelPath.0 | EvtQueryReverseDirection.0;
    let result = unsafe {
        EvtQuery(
            None,
            PCWSTR(channel_w.as_ptr()),
            PCWSTR(xpath_w.as_ptr()),
            flags,
        )
    }
    .map_err(|e| e.message())?;

    let mut events = Vec::new();
    let mut handles = [0isize; 64];
    'batches: loop {
        let mut returned = 0u32;
        // Ein Fehler bedeutet hier fast immer ERROR_NO_MORE_ITEMS.
        if unsafe { EvtNext(result, &mut handles, 1000, 0, &mut returned) }.is_err() {
            break;
        }
        for &raw in &handles[..returned as usize] {
            let handle = EVT_HANDLE(raw);
            if let Some(ev) = render_xml(handle).as_deref().and_then(parse) {
                events.push(ev);
            }
            unsafe {
                let _ = EvtClose(handle);
            }
            if events.len() >= LIMIT_PER_QUERY {
                break 'batches;
            }
        }
    }
    unsafe {
        let _ = EvtClose(result);
    }
    Ok(events)
}

fn render_xml(handle: EVT_HANDLE) -> Option<String> {
    let (mut used, mut count) = (0u32, 0u32);
    let _ = unsafe {
        EvtRender(
            None,
            handle,
            EvtRenderEventXml.0,
            0,
            None,
            &mut used,
            &mut count,
        )
    };
    if used == 0 {
        return None;
    }
    let mut buf = vec![0u16; (used as usize).div_ceil(2)];
    unsafe {
        EvtRender(
            None,
            handle,
            EvtRenderEventXml.0,
            (buf.len() * 2) as u32,
            Some(buf.as_mut_ptr().cast()),
            &mut used,
            &mut count,
        )
    }
    .ok()?;
    Some(from_wide(&buf))
}

fn child<'a, 'i>(node: roxmltree::Node<'a, 'i>, name: &str) -> Option<roxmltree::Node<'a, 'i>> {
    node.children()
        .find(|c| c.is_element() && c.tag_name().name() == name)
}

fn parse(xml: &str) -> Option<RawEvent> {
    let doc = roxmltree::Document::parse(xml).ok()?;
    let root = doc.root_element();
    let system = child(root, "System")?;
    let provider = child(system, "Provider")?.attribute("Name")?.to_string();
    let id = child(system, "EventID")?.text()?.trim().parse().ok()?;
    let time = child(system, "TimeCreated")?.attribute("SystemTime")?;
    let time = DateTime::parse_from_rfc3339(time).ok()?.with_timezone(&Utc);

    let mut data = Vec::new();
    if let Some(event_data) = child(root, "EventData") {
        for (i, d) in event_data.children().filter(|n| n.is_element()).enumerate() {
            let name = d
                .attribute("Name")
                .map(str::to_string)
                .unwrap_or_else(|| i.to_string());
            data.push((name, d.text().unwrap_or("").trim().to_string()));
        }
    }
    if let Some(user_data) = child(root, "UserData") {
        for d in user_data
            .descendants()
            .filter(|n| n.is_element() && !n.children().any(|c| c.is_element()))
        {
            data.push((
                d.tag_name().name().to_string(),
                d.text().unwrap_or("").trim().to_string(),
            ));
        }
    }
    Some(RawEvent {
        provider,
        id,
        time,
        data,
    })
}

/// SID (z. B. "S-1-5-21-…") in "DOMÄNE\Benutzer" auflösen, mit Cache.
fn account_name(sid: &str, cache: &mut HashMap<String, String>) -> String {
    if sid.is_empty() {
        return String::new();
    }
    cache
        .entry(sid.to_string())
        .or_insert_with(|| lookup_sid(sid).unwrap_or_else(|| sid.to_string()))
        .clone()
}

fn lookup_sid(sid: &str) -> Option<String> {
    let sid_w = wide(sid);
    let mut psid = PSID::default();
    unsafe { ConvertStringSidToSidW(PCWSTR(sid_w.as_ptr()), &mut psid) }.ok()?;
    let mut name = [0u16; 256];
    let mut domain = [0u16; 256];
    let (mut name_len, mut domain_len) = (name.len() as u32, domain.len() as u32);
    let mut sid_use = SID_NAME_USE::default();
    let found = unsafe {
        LookupAccountSidW(
            PCWSTR::null(),
            psid,
            Some(PWSTR(name.as_mut_ptr())),
            &mut name_len,
            Some(PWSTR(domain.as_mut_ptr())),
            &mut domain_len,
            &mut sid_use,
        )
    };
    unsafe {
        let _ = LocalFree(Some(HLOCAL(psid.0)));
    }
    found.ok()?;
    let (name, domain) = (from_wide(&name), from_wide(&domain));
    Some(if domain.is_empty() {
        name
    } else {
        format!("{domain}\\{name}")
    })
}
