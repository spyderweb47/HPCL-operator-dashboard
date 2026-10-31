fn main() {
    // Windows 7 build (target x86_64-win7-windows-msvc): a few imports only exist on Windows 8+.
    // Their DLLs are delay-loaded, and src/win7.rs stands in for whatever Windows 7 lacks.
    if std::env::var("CARGO_CFG_TARGET_VENDOR").as_deref() == Ok("win7") {
        for dll in ["combase.dll", "advapi32.dll", "api-ms-win-core-winrt-error-l1-1-0.dll"] {
            println!("cargo:rustc-link-arg-bins=/DELAYLOAD:{dll}");
        }
        println!("cargo:rustc-link-arg-bins=delayimp.lib");
    }
    tauri_build::build()
}
