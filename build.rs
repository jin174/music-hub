fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() == "windows" {
        let mut res = winres::WindowsResource::new();
        res.set_icon("icon.ico");
        res.set("ProductName", "Music Hub");
        res.set("FileDescription", "Music Hub Desktop");
        res.set("LegalCopyright", "Copyright (C) 2026");
        let _ = res.compile();
    }
}
