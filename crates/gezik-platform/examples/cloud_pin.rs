//! Asks the cloud app to keep a file here or free up its space, as Gezik's command does:
//! `cargo run -p gezik-platform --example cloud_pin -- keep|free <path>`. Changes only what
//! the cloud app keeps on this device; try it on a file of your own in a cloud folder.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [verb, path] = args.as_slice() else {
        eprintln!("usage: cloud_pin keep|free <path>");
        std::process::exit(2);
    };
    let path = std::path::Path::new(path);
    let result = match verb.as_str() {
        "keep" => gezik_platform::cloud_pin::keep(path),
        "free" => gezik_platform::cloud_pin::free_up(path),
        _ => {
            eprintln!("keep or free");
            std::process::exit(2);
        }
    };
    match result {
        Ok(()) => println!("asked"),
        Err(err) if gezik_platform::cloud_pin::is_left_alone(&err) => println!("left alone: {err}"),
        Err(err) => println!("failed: {err}"),
    }
}
