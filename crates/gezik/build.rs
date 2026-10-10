fn main() {
    slint_build::compile("ui/app.slint").expect("failed to compile Slint UI");
    // macOS names an app without a bundle by its Info.plist when the executable carries one:
    // the menu bar then says "Gezik" (and "About/Hide/Quit Gezik"), not the file name.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        let plist = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("macos/Info.plist");
        println!("cargo:rerun-if-changed={}", plist.display());
        println!("cargo:rustc-link-arg-bins=-Wl,-sectcreate,__TEXT,__info_plist,{}", plist.display());
    }
    // WinHTTP (downloading 7-Zip, ffmpeg, pdfium) is loaded on its first call, not at start:
    // loaded with the exe it cost ~50 KB of idle memory in every session.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
    {
        println!("cargo:rustc-link-arg-bins=/DELAYLOAD:winhttp.dll");
        println!("cargo:rustc-link-arg-bins=delayimp.lib");
        // The exe's own imports come from System32 only, never from its folder: the
        // administrator helper is this exe (9b7 security review 1; the CRT is static).
        println!("cargo:rustc-link-arg-bins=/DEPENDENTLOADFLAG:0x800");
    }
}
