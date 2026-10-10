// PROBE_STATIC=1 builds the window with two literal PaneView instances instead of the
// `for` over the model, to compare the exe (does a second instance add code?).
fn main() {
    println!("cargo:rerun-if-env-changed=PROBE_STATIC");
    println!("cargo:rerun-if-changed=app.slint");
    println!("cargo:rerun-if-changed=pane.slint");
    if std::env::var("PROBE_STATIC").is_err() {
        slint_build::compile("app.slint").unwrap();
        return;
    }
    let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap().replace('\\', "/");
    let src = std::fs::read_to_string("app.slint").unwrap().replace("\r\n", "\n");
    let start = src.find("            for p[i]").unwrap();
    let close = "\n            }\n";
    let end = src[start..].find(close).unwrap() + start + close.len();
    let block = &src[start..end];
    let one = |n: usize| {
        block
            .replace("for p[i] in root.panes: PaneView {", &format!("PaneView {{ visible: root.panes.length > {n};"))
            .replace("data: p;", &format!("data: root.panes[{n}];"))
            .replace("index: i;", &format!("index: {n};"))
    };
    let out = src[..start].to_string() + &one(0) + &one(1) + &src[end..];
    let out = out.replace("\"pane.slint\"", &format!("\"{dir}/pane.slint\""));
    let path = std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("static.slint");
    std::fs::write(&path, out).unwrap();
    slint_build::compile(&path).unwrap();
}
