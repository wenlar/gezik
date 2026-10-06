fn main() {
    slint_build::compile("ui/app.slint").expect("failed to compile Slint UI");
    // macOS names an app without a bundle by its Info.plist when the executable carries one:
    // the menu bar then says "Gezik" (and "About/Hide/Quit Gezik"), not the file name.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        let plist = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("macos/Info.plist");
        println!("cargo:rerun-if-changed={}", plist.display());
        println!("cargo:rustc-link-arg-bins=-Wl,-sectcreate,__TEXT,__info_plist,{}", plist.display());
    }
}
