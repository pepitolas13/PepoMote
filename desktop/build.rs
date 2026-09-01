fn main() {
    // Icono y metadatos de versión del exe (solo target Windows)
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("app.ico");
        res.set("ProductName", "PepoMote");
        res.set("FileDescription", "PepoMote — tu móvil, mando de tu PC");
        res.set("LegalCopyright", "GPL-3.0-or-later · PepoTech");
        res.set("OriginalFilename", "PepoMote.exe");
        if let Err(e) = res.compile() {
            println!("cargo:warning=sin recursos de Windows: {e}");
        }
    }
}
