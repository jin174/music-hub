fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() == "windows" {
        let mut res = winres::WindowsResource::new();
        res.set_icon("icon.ico");
        res.set("ProductName", "Music Hub");
        res.set("FileDescription", "Music Hub Desktop");
        res.set("LegalCopyright", "Copyright (C) 2026");
        res.set_manifest(
            r#"<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0" xmlns:asmv3="urn:schemas-microsoft-com:asm.v3">
    <asmv3:application>
        <asmv3:windowsSettings>
            <dpiAware xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">true</dpiAware>
            <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2</dpiAwareness>
        </asmv3:windowsSettings>
    </asmv3:application>
    <dependency>
        <dependentAssembly>
            <assemblyIdentity type="win32"
                name="Microsoft.Windows.Common-Controls"
                version="6.0.0.0" processorArchitecture="*"
                publicKeyToken="6595b64144ccf1df" />
        </dependentAssembly>
    </dependency>
</assembly>"#,
        );
        if let Err(err) = res.compile() {
            println!("cargo:warning=winres failed: {err}");
        }

        // winres only emits `cargo:rustc-link-lib=dylib=resource`, and the MSVC linker
        // does not pull a resource-only archive in through -l on this toolchain: the
        // binary shipped with no icon, no version info and — critically — no manifest,
        // so comctl32 v6 was never activated and the TaskDialogIndirect import pulled
        // in by the gpui dependency chain failed at load time with "entry point not
        // found". Hand the archive to the linker for the binary target explicitly.
        let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR is set for build scripts");
        let resource_lib = std::path::Path::new(&out_dir).join("resource.lib");
        if resource_lib.exists() {
            println!("cargo:rustc-link-arg-bins={}", resource_lib.display());
        } else {
            println!(
                "cargo:warning=resource.lib was not produced; the binary will have no icon, version info or manifest"
            );
        }
    }
}
