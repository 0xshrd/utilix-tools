// Schreibt die Datei-Metadaten in die .exe. SignPath prüft, dass ProductName "utilix" ist.
fn main() {
    // Lokal mit der GNU-Toolchain fehlt windres; Release-Builds laufen mit MSVC.
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("msvc") {
        return;
    }
    let mut res = winresource::WindowsResource::new();
    res.set("ProductName", "utilix");
    res.set("FileDescription", env!("CARGO_PKG_DESCRIPTION"));
    res.set("LegalCopyright", "utilix contributors, GPL-3.0-or-later");
    res.compile()
        .expect("Windows-Ressourcen konnten nicht eingebettet werden");
}
