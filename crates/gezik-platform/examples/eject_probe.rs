//! Lists the drives, what Gezik would offer for each, and (Windows) the canonical verbs of
//! each drive's Explorer menu. Ejects nothing: no verb is run.
//! `cargo run -p gezik-platform --example eject_probe`

use gezik_platform::eject;

fn main() {
    for drive in gezik_platform::drives() {
        let way = eject::way_for(&drive.kind, eject::is_system(&drive), cfg!(windows), cfg!(target_os = "macos"));
        println!("{} {:?} {:?} -> {:?}", drive.path.display(), drive.label, drive.kind, way.map(|w| w.title()));
        #[cfg(windows)]
        println!("  verbs: {:?}", eject::shell_verbs(&drive.path));
    }
}
