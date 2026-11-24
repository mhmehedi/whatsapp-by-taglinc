// Windows: embed the app icon and version info into the .exe.
fn main() {
    println!("cargo:rerun-if-changed=assets/AppIcon.ico");
    // Only when building on Windows (the resource compiler isn't available elsewhere).
    let host_windows = std::env::var("HOST").is_ok_and(|h| h.contains("windows"));
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") && host_windows {
        let mut res = winresource::WindowsResource::new();
        res.set_icon_with_id("assets/AppIcon.ico", "1");
        res.set("ProductName", "Whatsapp by TagLinc");
        res.set("FileDescription", "Whatsapp by TagLinc");
        res.set("CompanyName", "TagLinc");
        res.set("LegalCopyright", "Copyright (c) 2026 TagLinc");
        res.compile().expect("failed to embed Windows resources");
    }
}
