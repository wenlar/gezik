//! Prints the cloud roots Gezik finds and, for each path given, the cloud state the listing
//! would show: `cargo run -p gezik-platform --example cloud_probe -- [path…]`. Reads only.

fn main() {
    let roots = gezik_platform::cloud::roots();
    println!("{} cloud root(s):", roots.len());
    for root in &roots {
        println!("  {:<24} {:<16} {}", root.label, root.account, root.path.display());
    }
    for arg in std::env::args().skip(1) {
        let path = std::path::PathBuf::from(&arg);
        let under = gezik_platform::cloud::root_of(&roots, &path).map(|r| r.label.clone());
        match std::fs::symlink_metadata(&path) {
            Ok(meta) => {
                let flags = gezik_core::attribute_flags(&meta);
                #[cfg(windows)]
                let raw = format!("{:#x}", std::os::windows::fs::MetadataExt::file_attributes(&meta));
                #[cfg(not(windows))]
                let raw = String::from("-");
                println!("{arg}: under {under:?}, attributes {raw}, state {:?}", gezik_core::cloud_state(flags));
            }
            Err(err) => println!("{arg}: {err}"),
        }
    }
}
