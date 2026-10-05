//! A stand-in for a user command's program in `tests/convert_tasks.rs` (built by `cargo test`
//! with the other examples). Not shipped.
//!
//! - `copy <in> <out>`: copies `in` to `out`.
//! - `append <file>`: adds ` changed` to the end of `file` (a command that edits in place).
//! - `args <out> <arg>…`: writes the arguments after `out` to it, one per line, and the
//!   folder it runs in as the last line.
//! - `fail`: prints three error lines and exits with 3.
//! - `nothing`: exits with 0 and writes nothing.
//! - `hang`: waits until killed.

use std::io::Write;

fn main() {
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    let mode = args.first().and_then(|a| a.to_str()).unwrap_or("");
    match mode {
        "copy" => {
            std::fs::copy(&args[1], &args[2]).unwrap();
        }
        "append" => {
            let mut file = std::fs::OpenOptions::new().append(true).open(&args[1]).unwrap();
            file.write_all(b" changed").unwrap();
        }
        "args" => {
            let mut text = String::new();
            for arg in &args[2..] {
                text.push_str(&arg.to_string_lossy());
                text.push('\n');
            }
            text.push_str(&std::env::current_dir().unwrap().to_string_lossy());
            std::fs::write(&args[1], text).unwrap();
        }
        "fail" => {
            for n in 1..=3 {
                eprintln!("fake tool error {n}");
            }
            std::process::exit(3);
        }
        "nothing" => {}
        "hang" => loop {
            std::thread::sleep(std::time::Duration::from_secs(1));
        },
        other => {
            eprintln!("fake tool: unknown mode {other:?}");
            std::process::exit(9);
        }
    }
}
