//! Packs the built-in languages (`lang/*.po`) into the exe (spec 15 §4.3): the template's keys
//! once as sorted hashes, each language's translations in that order, deflated. Only the
//! language in use is inflated at run time.

#[path = "src/blob.rs"]
mod blob;
#[path = "src/po.rs"]
#[allow(dead_code)]
mod po;

use std::collections::HashMap;
use std::fmt::Write;
use std::path::Path;

fn main() {
    let dir = Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("lang");
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    println!("cargo:rerun-if-changed={}", dir.display());
    let read = |path: &Path| {
        let text = std::fs::read_to_string(path).unwrap_or_default();
        po::parse(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
    };
    let pot = read(&dir.join("gezik.pot"));
    let mut keys: Vec<u64> = pot.entries.iter().map(|e| blob::hash(&e.key())).collect();
    keys.sort_unstable();
    keys.dedup();

    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .map(|entries| entries.filter_map(Result::ok).map(|e| e.path()).collect())
        .unwrap_or_default();
    files.retain(|p| p.extension().is_some_and(|e| e == "po"));
    files.sort();
    let mut code_out =
        format!("pub static KEYS: &[u64] = &{keys:?};\npub static BUILTINS: &[crate::catalog::Builtin] = &[\n");
    for path in files {
        let file = read(&path);
        let code = path.file_stem().unwrap().to_string_lossy().into_owned();
        let mut translations = HashMap::new();
        let (mut translated, mut fuzzy) = (0, 0);
        for entry in file.entries.iter().filter(|e| e.translated()) {
            let hash = blob::hash(&entry.key());
            if keys.binary_search(&hash).is_ok() {
                translated += 1;
                fuzzy += usize::from(entry.fuzzy);
                translations.insert(hash, entry.strs.clone());
            }
        }
        let packed = blob::pack(&keys, &translations);
        let bin = out.join(format!("lang-{code}.bin"));
        std::fs::write(&bin, miniz_oxide::deflate::compress_to_vec(packed.as_bytes(), 10)).unwrap();
        let name = file.field("X-Gezik-Name").unwrap_or(&code).to_owned();
        let human = file.field("X-Gezik-Review") != Some("machine");
        let plural = file.field("Plural-Forms").unwrap_or_default().to_owned();
        writeln!(
            code_out,
            "    crate::catalog::Builtin {{ code: {code:?}, name: {name:?}, human: {human}, plural: {plural:?}, \
             translated: {translated}, fuzzy: {fuzzy}, data: include_bytes!({:?}) }},",
            bin.display().to_string()
        )
        .unwrap();
    }
    code_out.push_str("];\n");
    std::fs::write(out.join("embedded.rs"), code_out).unwrap();
}
