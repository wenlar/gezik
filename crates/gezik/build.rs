fn main() {
    // `@tr` has only the contexts written in it (spec 15 §4.2), as `gezik-lang extract` reads
    // them: by default Slint would give each one its component's name.
    let config = slint_build::CompilerConfiguration::new()
        .with_default_translation_context(slint_build::DefaultTranslationContext::None);
    slint_build::compile_with_config("ui/app.slint", config).expect("failed to compile Slint UI");
    // The `measure` profile (Cargo.toml) gets `gezik_measure`: like debug builds it reads the
    // update feed's test override (update.rs). Its build folder is `<target>/[<triple>/]measure/`.
    println!("cargo::rustc-check-cfg=cfg(gezik_measure)");
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    if out.ancestors().nth(3).and_then(|p| p.file_name()).is_some_and(|name| name == "measure") {
        println!("cargo::rustc-cfg=gezik_measure");
    }
    // macOS names an app without a bundle by its Info.plist when the executable carries one:
    // the menu bar then says "Gezik" (and "About/Hide/Quit Gezik"), not the file name.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        let plist = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("macos/Info.plist");
        println!("cargo:rerun-if-changed={}", plist.display());
        println!("cargo:rustc-link-arg-bins=-Wl,-sectcreate,__TEXT,__info_plist,{}", plist.display());
    }
    // The exe's icon (Explorer, the taskbar before the window opens, shortcuts); the window's own
    // icon is set in the .slint files.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rerun-if-changed=icons/gezik.ico");
        embed_resource::compile("icons/gezik.rc", embed_resource::NONE).manifest_optional().unwrap();
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
        // The segment heap: it gives freed pages back by itself, where the default heap kept
        // ~1 MB after the second pane closed and ~1.5 MB after a 30-folder walk (spec 10 §4.2).
        let manifest =
            std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("windows/gezik.manifest");
        println!("cargo:rerun-if-changed={}", manifest.display());
        println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg-bins=/MANIFESTINPUT:{}", manifest.display());
    }
}
