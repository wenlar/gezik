//! The texts of one language (spec 15 §4.1): a file dropped in the lang folder over the
//! built-in language over the source (English). A text one layer lacks comes from the next.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

use crate::blob;
use crate::plural::{self, Plural};

/// A built-in language as `build.rs` packed it.
#[derive(Debug)]
pub struct Builtin {
    pub code: &'static str,
    pub name: &'static str,
    /// `X-Gezik-Review: human` (or none); `machine` while machine translations wait for review.
    pub human: bool,
    /// `Plural-Forms`; empty for the table's.
    pub plural: &'static str,
    /// Texts translated, and of those not reviewed (`fuzzy`).
    pub translated: usize,
    pub fuzzy: usize,
    /// The deflated translations (`blob`).
    pub data: &'static [u8],
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Texts {
    /// A dropped file's: catalogue key → forms.
    Map(HashMap<String, Vec<String>>),
    /// A built-in language's, inflated: entries in `keys`' order, where each starts.
    Packed { keys: &'static [u64], text: String, starts: Vec<u32> },
}

/// One language's translations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layer {
    /// Its name in its own language (`X-Gezik-Name`), or its code.
    pub name: String,
    pub human: bool,
    pub translated: usize,
    pub fuzzy: usize,
    plural: Plural,
    texts: Texts,
}

impl Layer {
    /// A dropped file's translations (`check::load` makes it).
    pub(crate) fn from_map(name: String, human: bool, plural: Plural, map: HashMap<String, Vec<String>>) -> Layer {
        Layer { name, human, translated: map.len(), fuzzy: 0, plural, texts: Texts::Map(map) }
    }

    /// A built-in language, inflated (its translations against `keys`).
    pub fn packed(builtin: &Builtin, keys: &'static [u64]) -> Option<Layer> {
        let bytes = miniz_oxide::inflate::decompress_to_vec_with_limit(builtin.data, crate::po::MAX_BYTES).ok()?;
        let text = String::from_utf8(bytes).ok()?;
        let mut starts = Vec::with_capacity(keys.len());
        starts.push(0);
        starts.extend(text.match_indices(blob::END).map(|(i, _)| u32::try_from(i + 1).unwrap_or(u32::MAX)));
        if starts.len() != keys.len().max(1) {
            return None;
        }
        let plural = match builtin.plural {
            "" => plural::builtin(builtin.code).and_then(|p| Plural::parse(p).ok()).unwrap_or_default(),
            header => Plural::parse(header).unwrap_or_default(),
        };
        Some(Layer {
            name: builtin.name.to_owned(),
            human: builtin.human,
            translated: builtin.translated,
            fuzzy: builtin.fuzzy,
            plural,
            texts: Texts::Packed { keys, text, starts },
        })
    }

    /// Form `index` of `key`'s translation, if it has one.
    fn form(&self, key: &str, index: usize) -> Option<&str> {
        let found = match &self.texts {
            Texts::Map(map) => map.get(key)?.get(index).map(String::as_str),
            Texts::Packed { keys, text, starts } => {
                let at = keys.binary_search(&blob::hash(key)).ok()?;
                let start = *starts.get(at)? as usize;
                let end = starts.get(at + 1).map_or(text.len(), |&next| next as usize - 1);
                text.get(start..end)?.split(blob::FORM).nth(index)
            }
        };
        found.filter(|s| !s.is_empty())
    }
}

/// The language in use: its layers (a dropped file first), or the pseudo language.
#[derive(Debug)]
pub struct Catalog {
    pub code: String,
    pseudo: bool,
    layers: Vec<Arc<Layer>>,
}

impl Catalog {
    pub fn pseudo() -> Catalog {
        Catalog { code: crate::lang::PSEUDO.to_owned(), pseudo: true, layers: Vec::new() }
    }

    /// `code` from the dropped file `file` (if any) over the built-in one; `None` when neither
    /// has it (English: nothing to load). Only this language is inflated.
    pub fn open(code: &str, file: Option<Arc<Layer>>) -> Option<Catalog> {
        if code == crate::lang::PSEUDO {
            return Some(Catalog::pseudo());
        }
        let builtin = crate::builtins()
            .iter()
            .find(|b| b.code == code)
            .and_then(|b| Layer::packed(b, crate::keys()))
            .map(Arc::new);
        Catalog::layered(code, file.into_iter().chain(builtin).collect())
    }

    pub(crate) fn layered(code: &str, layers: Vec<Arc<Layer>>) -> Option<Catalog> {
        (!layers.is_empty()).then(|| Catalog { code: code.to_owned(), pseudo: false, layers })
    }

    /// `id` (in `context`) in this language.
    pub fn get<'a>(&'a self, context: &str, id: &'a str) -> Cow<'a, str> {
        if self.pseudo {
            return Cow::Owned(crate::pseudo::pseudo(id));
        }
        let key = crate::po::key(context, id);
        self.layers.iter().find_map(|layer| layer.form(&key, 0)).map_or(Cow::Borrowed(id), Cow::Borrowed)
    }

    /// The form of `singular`/`plural` (in `context`) that `n` takes in this language.
    pub fn nget<'a>(&'a self, context: &str, singular: &'a str, plural: &'a str, n: u64) -> Cow<'a, str> {
        let english = if n == 1 { singular } else { plural };
        if self.pseudo {
            return Cow::Owned(crate::pseudo::pseudo(english));
        }
        let key = crate::po::key(context, singular);
        let found = self.layers.iter().find_map(|layer| layer.form(&key, layer.plural.index(n)));
        found.map_or(Cow::Borrowed(english), Cow::Borrowed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(pairs: &[(&str, &[&str])]) -> HashMap<String, Vec<String>> {
        pairs.iter().map(|(k, v)| ((*k).to_owned(), v.iter().map(|s| (*s).to_owned()).collect())).collect()
    }

    /// A built-in language packed as `build.rs` does it.
    fn builtin(entries: &[(&str, &[&str])]) -> (Builtin, &'static [u64]) {
        let mut keys: Vec<u64> = entries.iter().map(|(k, _)| blob::hash(k)).collect();
        keys.push(blob::hash("not translated"));
        keys.sort_unstable();
        let translations =
            entries.iter().map(|(k, v)| (blob::hash(k), v.iter().map(|s| (*s).to_owned()).collect())).collect();
        let packed = blob::pack(&keys, &translations);
        let data = miniz_oxide::deflate::compress_to_vec(packed.as_bytes(), 10).leak();
        let b = Builtin { code: "tr", name: "Türkçe", human: true, plural: "", translated: 2, fuzzy: 0, data };
        (b, keys.leak())
    }

    #[test]
    fn a_dropped_file_over_the_built_in_over_english() {
        let (b, keys) = builtin(&[
            ("Open", &["Aç"]),
            ("Close", &["Kapat"]),
            ("{n} file", &["{n} dosya"]),
            ("menu\u{4}Open", &["Açın"]),
        ]);
        let packed = Layer::packed(&b, keys).unwrap();
        assert_eq!(packed.plural.count, 1, "Turkish from the table");
        let file = Layer::from_map("Türkçe".into(), true, Plural::default(), map(&[("Close", &["Kapa"])]));
        let catalog = Catalog::layered("tr", vec![Arc::new(file), Arc::new(packed)]).unwrap();
        assert_eq!(catalog.get("", "Close"), "Kapa", "the file first");
        assert_eq!(catalog.get("", "Open"), "Aç", "then the built-in");
        assert_eq!(catalog.get("menu", "Open"), "Açın", "by context");
        assert_eq!(catalog.get("", "not translated"), "not translated", "an empty one: English");
        assert_eq!(catalog.get("", "Unknown"), "Unknown", "then English");
        assert!(matches!(catalog.get("", "Unknown"), Cow::Borrowed(_)));
        assert_eq!(catalog.nget("", "{n} file", "{n} files", 5), "{n} dosya");
        assert_eq!(catalog.nget("", "{n} thing", "{n} things", 5), "{n} things");
        assert_eq!(catalog.nget("", "{n} thing", "{n} things", 1), "{n} thing");
    }

    #[test]
    fn plurals_of_a_dropped_file() {
        let polish =
            Plural::parse("nplurals=3; plural=(n==1 ? 0 : n%10>=2 && n%10<=4 && (n%100<10 || n%100>=20) ? 1 : 2);");
        let file = Layer::from_map(
            "Polski".into(),
            true,
            polish.unwrap(),
            map(&[("{n} file", &["{n} plik", "{n} pliki", "{n} plików"])]),
        );
        let catalog = Catalog::layered("pl", vec![Arc::new(file)]).unwrap();
        let forms: Vec<_> = [1, 2, 5, 22, 112].iter().map(|&n| catalog.nget("", "{n} file", "{n} files", n)).collect();
        assert_eq!(forms, ["{n} plik", "{n} pliki", "{n} plików", "{n} pliki", "{n} plików"]);
    }

    #[test]
    fn pseudo_and_nothing() {
        let pseudo = Catalog::pseudo();
        assert_eq!(pseudo.get("", "Open {}"), crate::pseudo::pseudo("Open {}"));
        assert_eq!(pseudo.nget("", "{n} file", "{n} files", 2), crate::pseudo::pseudo("{n} files"));
        assert!(Catalog::open("en", None).is_none(), "English loads nothing");
        assert!(Catalog::open("tr", None).is_none(), "no built-in languages yet");
        let file = Arc::new(Layer::from_map("Polski".into(), true, Plural::default(), HashMap::new()));
        assert_eq!(Catalog::open("pl", Some(file)).unwrap().code, "pl");
        assert_eq!(Catalog::open("pseudo", None).unwrap().code, "pseudo");
    }

    #[test]
    fn a_damaged_blob_is_refused() {
        let (mut b, keys) = builtin(&[("Open", &["Aç"])]);
        b.data = b"not deflate";
        assert!(Layer::packed(&b, keys).is_none());
        let packed = miniz_oxide::deflate::compress_to_vec(b"a\0b\0c", 10).leak();
        b.data = packed;
        assert!(Layer::packed(&b, keys).is_none(), "three entries for two keys");
    }
}
