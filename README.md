# utilix-tools

Portable Windows-Tools für Netzwerk, System, Hardware und Forensik. Jedes Tool ist eine einzelne `.exe` ohne Installer, läuft vom USB-Stick und hinterlässt keine Spuren in der Registry.

Website: https://utils.grand-analytics.de · Quellcode der Website: [0xshrd/utilix](https://github.com/0xshrd/utilix)

## Tools

| Tool | Aufgabe | Stand |
| --- | --- | --- |
| LastActivity | Zeitachse der PC-Nutzung: Programmstarts, geöffnete Dateien, Start/Herunterfahren, Anmeldungen, Abstürze, Netzwerk, Installationen | in Arbeit |
| NetScope | Offene Ports & Verbindungen mit Prozess, Remote-Host und Datenrate | geplant |

Weitere Tools (WifiLens, DNSWatch, ProcPeek, EventGlass, CrashView, USBTrail, BlueScan, DriveDeck, FileTrace) sind geplant.

### LastActivity

Liest lokal und nur lesend:

| Quelle | Aktionen |
| --- | --- |
| Ereignisprotokolle (System, Application, NetworkProfile, WLAN-AutoConfig, TaskScheduler) | Start, Herunterfahren, unerwarteter Neustart, Energiesparmodus, An-/Abmeldung, Absturz, Hänger, Bluescreen, Windows Installer, Wiederherstellungspunkt, Netzwerk/WLAN, geplante Aufgaben |
| Prefetch (`C:\Windows\Prefetch`, braucht Adminrechte) | Programmstarts mit bis zu 8 Zeitpunkten pro Programm |
| Zuletzt verwendet (`%APPDATA%\Microsoft\Windows\Recent`) | Geöffnete Dateien und Ordner |
| ShellBags (Registry, `BagMRU`) | Im Explorer angesehene Ordner, auch auf USB-Laufwerken und in ZIP-Archiven |
| Öffnen/Speichern-Dialoge (Registry, `OpenSavePidlMRU`) | Zuletzt gewählte Datei je Dateiendung |
| Registry (Uninstall-Schlüssel) | Installierte Software |
| Archivierte Ereignisprotokolle (`Archive-*.evtx`) | wie oben, falls Windows Protokolle archiviert |

Zeitpunkte aus der Registry (ShellBags, Dialoge) sind die letzte Schreibzeit des Schlüssels und daher Näherungswerte.

Export ohne Oberfläche: `lastactivity.exe --csv aktivitaet.csv --json aktivitaet.json --html bericht.html`

## Download

Fertige Versionen liegen unter [Releases](https://github.com/0xshrd/utilix-tools/releases), jeweils als ZIP mit `SHA256SUMS.txt`. Prüfen:

```powershell
Get-FileHash .\lastactivity-v0.1.1-x64.zip -Algorithm SHA256
```

Die Programme sind noch nicht digital signiert. Windows SmartScreen kann deshalb beim ersten Start warnen („Weitere Informationen“ → „Trotzdem ausführen“).

## Selbst bauen

Voraussetzungen: [Rust](https://rustup.rs) und die Visual Studio Build Tools (Workload „Desktopentwicklung mit C++“). Die Rust-Version ist in `rust-toolchain.toml` festgelegt.

```powershell
cargo build --release
```

Die Binaries landen in `target\release`. Offizielle Releases baut ausschließlich GitHub Actions ([build.yml](.github/workflows/build.yml)) aus einem getaggten Commit (`v*`).

## Code signing policy

<!-- Erst nach Zusage durch die SignPath Foundation einkommentieren:
Free code signing provided by [SignPath.io](https://about.signpath.io), certificate by [SignPath Foundation](https://signpath.org).
-->

Releases werden ausschließlich aus diesem Repository über GitHub Actions gebaut. Jede Signatur muss von einem Approver manuell freigegeben werden.

**Team und Rollen**

| Rolle | Mitglieder |
| --- | --- |
| Committer und Reviewer | [Mitglieder von 0xshrd](https://github.com/orgs/0xshrd/people) |
| Approver | [Owner von 0xshrd](https://github.com/orgs/0xshrd/people?query=role%3Aowner) |

Pull Requests von Personen außerhalb des Teams werden vor dem Merge von einem Reviewer geprüft. Alle Teammitglieder nutzen Zwei-Faktor-Authentifizierung für GitHub und SignPath.

**Datenschutz**

Dieses Programm überträgt keine Informationen an andere vernetzte Systeme, es sei denn, der Benutzer fordert dies ausdrücklich an.

*This program will not transfer any information to other networked systems unless specifically requested by the user or the person installing or operating it.*

## Lizenz

[GPL-3.0-or-later](LICENSE). Copyright © utilix contributors.
