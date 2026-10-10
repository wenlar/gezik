//! Reads server addresses and (Windows) lists a server's shares; connects to nothing.
//! `cargo run -p gezik-platform --example network_probe -- '\\nas' 'smb://nas/Foto Arşivi'`

fn main() {
    let windows = cfg!(windows);
    for text in std::env::args().skip(1) {
        match gezik_platform::network::parse_address(&text, windows) {
            Err(why) => println!("{text:?}: {why}"),
            Ok(address) => {
                println!(
                    "{text:?}: unc {} | url {} | rest {:?}",
                    address.unc(),
                    address.smb_url(),
                    address.rest_path()
                );
                if address.share.is_empty() {
                    match gezik_platform::network::shares(&address.server) {
                        Ok(names) => println!("  shares: {names:?}"),
                        Err(err) => println!("  shares: {err}"),
                    }
                }
            }
        }
    }
    if windows {
        println!("free letter: {:?}", gezik_platform::network::free_letter(gezik_platform::drive_signature() as u32));
    }
}
