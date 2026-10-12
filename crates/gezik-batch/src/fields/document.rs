//! Office documents: OOXML (`docProps/core.xml`, `docProps/app.xml`) and OpenDocument
//! (`meta.xml`), each part read from the ZIP up to a bound once unpacked; a small tag finder,
//! no XML parser.

use std::io::{Read, Seek};

use gezik_core::fields as f;

use super::Out;

/// The most of one part read, unpacked.
const PART_MAX: u64 = 256 * 1024;

pub(super) fn read<R: Read + Seek>(r: R, out: &mut Out) {
    let Ok(mut zip) = zip::ZipArchive::new(r) else { return };
    let mut part = |name: &str| {
        let file = zip.by_name(name).ok()?;
        let mut data = Vec::new();
        file.take(PART_MAX).read_to_end(&mut data).ok()?;
        Some(String::from_utf8_lossy(&data).into_owned())
    };
    if let Some(core) = part("docProps/core.xml") {
        out.text(f::TITLE, &tag(&core, "title").unwrap_or_default());
        out.text(f::AUTHOR, &tag(&core, "creator").unwrap_or_default());
        out.text(f::SUBJECT, &tag(&core, "subject").unwrap_or_default());
    }
    if let Some(app) = part("docProps/app.xml") {
        // A presentation counts slides.
        out.leading_int(f::PAGES, &tag(&app, "Pages").or_else(|| tag(&app, "Slides")).unwrap_or_default());
        out.leading_int(f::WORDS, &tag(&app, "Words").unwrap_or_default());
    }
    if let Some(meta) = part("meta.xml") {
        out.text(f::TITLE, &tag(&meta, "title").unwrap_or_default());
        out.text(f::AUTHOR, &tag(&meta, "initial-creator").or_else(|| tag(&meta, "creator")).unwrap_or_default());
        out.text(f::SUBJECT, &tag(&meta, "subject").unwrap_or_default());
        out.leading_int(f::PAGES, &attr(&meta, "page-count").unwrap_or_default());
        out.leading_int(f::WORDS, &attr(&meta, "word-count").unwrap_or_default());
    }
}

/// The text of the first element named `local` (any namespace prefix).
fn tag(xml: &str, local: &str) -> Option<String> {
    let mut rest = xml;
    while let Some(i) = rest.find('<') {
        rest = rest.get(i + 1..)?;
        // Names are short: a long run without an end is no tag (and is not scanned again and
        // again: 256 KB of `<` would take minutes).
        let Some((end, _)) = rest.char_indices().take(128).find(|(_, c)| *c == '>' || *c == '/' || c.is_whitespace())
        else {
            continue;
        };
        let name = rest.get(..end)?;
        if name.rsplit(':').next() == Some(local) {
            let open = rest.find('>')?;
            if rest.get(..open)?.ends_with('/') {
                return None;
            }
            let content = rest.get(open + 1..)?;
            return Some(unescape(content.get(..content.find('<')?)?));
        }
    }
    None
}

/// The value of the first attribute named `local` (any namespace prefix).
fn attr(xml: &str, local: &str) -> Option<String> {
    let key = format!("{local}=\"");
    let mut from = 0;
    while let Some(i) = xml.get(from..)?.find(&key) {
        let at = from + i;
        let before = xml.get(..at)?.chars().next_back();
        if before.is_some_and(|c| c == ':' || c.is_whitespace()) {
            let value = xml.get(at + key.len()..)?;
            return Some(unescape(value.get(..value.find('"')?)?));
        }
        from = at + key.len();
    }
    None
}

/// The five XML entities and character references.
fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find('&') {
        out.push_str(rest.get(..i).unwrap_or_default());
        rest = rest.get(i..).unwrap_or_default();
        let Some((end, _)) = rest.char_indices().take(11).find(|(_, c)| *c == ';') else {
            out.push('&');
            rest = rest.get(1..).unwrap_or_default();
            continue;
        };
        let name = rest.get(1..end).unwrap_or_default();
        let c = match name {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => name
                .strip_prefix("#x")
                .map(|hex| u32::from_str_radix(hex, 16))
                .or_else(|| name.strip_prefix('#').map(str::parse))
                .and_then(Result::ok)
                .and_then(char::from_u32),
        };
        match c {
            Some(c) => {
                out.push(c);
                rest = rest.get(end + 1..).unwrap_or_default();
            }
            None => {
                out.push('&');
                rest = rest.get(1..).unwrap_or_default();
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/fields").join(name)
    }

    #[test]
    fn office_and_open_document_properties() {
        let docx = super::super::read(&fixture("report.docx"), f::FieldSet::ALL, &|| false);
        let shown = |id| docx.text(id).unwrap_or_default();
        assert_eq!(shown(f::TITLE), "Yıllık Rapor & Özet");
        assert_eq!(shown(f::AUTHOR), "Ayşe Yılmaz");
        assert_eq!(shown(f::SUBJECT), "Bütçe");
        assert_eq!(shown(f::PAGES), "12");
        assert_eq!(shown(f::WORDS), "3,456");
        let odt = super::super::read(&fixture("notes.odt"), f::FieldSet::ALL, &|| false);
        let shown = |id| odt.text(id).unwrap_or_default();
        assert_eq!(shown(f::TITLE), "Toplantı Notları");
        assert_eq!(shown(f::AUTHOR), "Mehmet Kaya");
        assert_eq!(shown(f::SUBJECT), "Proje");
        assert_eq!(shown(f::PAGES), "3");
        assert_eq!(shown(f::WORDS), "512");
    }

    #[test]
    fn tags_attributes_and_entities() {
        assert_eq!(tag("<a:x/><b:x>v &lt;1&#x3E; &#65;&bogus;</b:x>", "x"), None, "the first is empty");
        assert_eq!(tag("<b:x>v &lt;1&#x3E; &#65;&bogus;</b:x>", "x").as_deref(), Some("v <1> A&bogus;"));
        assert_eq!(tag("<xy>1</xy><y>2</y>", "y").as_deref(), Some("2"));
        assert_eq!(attr("<m a:page-count=\"7\"/>", "page-count").as_deref(), Some("7"));
        assert_eq!(attr("<m xpage-count=\"7\"/>", "page-count"), None);
        assert_eq!(unescape("a & b &#xFFFFFFFF;"), "a & b &#xFFFFFFFF;");
    }

    #[test]
    fn long_runs_of_markup_are_quick() {
        for run in ["<", "&", "<a"] {
            let xml = run.repeat(256 * 1024 / run.len());
            let start = std::time::Instant::now();
            assert_eq!(tag(&xml, "title"), None);
            let _ = unescape(&xml);
            // Linear: milliseconds; the quadratic scan took minutes.
            assert!(start.elapsed() < std::time::Duration::from_secs(2), "{run}: {:?}", start.elapsed());
        }
    }

    #[test]
    fn hostile_documents_do_not_panic() {
        let seeds = [std::fs::read(fixture("report.docx")).unwrap(), std::fs::read(fixture("notes.odt")).unwrap()];
        super::super::tests::fuzz(&seeds, 4, &|r, _| read(r.clone(), &mut Out::new(f::FieldSet::ALL)));
    }
}
