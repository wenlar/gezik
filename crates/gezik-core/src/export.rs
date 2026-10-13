//! A list written as text, CSV or HTML (spec 11 §11): row by row into any writer, so a long
//! list is never held whole.

use std::borrow::Cow;
use std::io::{self, Write};
use std::path::Path;

use crate::batch::date::DateParts;
use crate::view::ColumnKey;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// One name or path per line.
    Text,
    Csv,
    Html,
}

impl Format {
    pub const ALL: [Format; 3] = [Format::Text, Format::Csv, Format::Html];

    pub fn extension(self) -> &'static str {
        match self {
            Format::Text => "txt",
            Format::Csv => "csv",
            Format::Html => "html",
        }
    }

    /// The format a file name's extension asks for (`.htm` too, any case).
    pub fn from_path(path: &Path) -> Option<Format> {
        let ext = path.extension()?.to_str()?.to_ascii_lowercase();
        match ext.as_str() {
            "htm" => Some(Format::Html),
            ext => Format::ALL.into_iter().find(|f| f.extension() == ext),
        }
    }

    /// Its name in state.toml (`[export] format`).
    pub fn as_str(self) -> &'static str {
        match self {
            Format::Text => "text",
            Format::Csv => "csv",
            Format::Html => "html",
        }
    }

    pub fn parse(text: &str) -> Option<Format> {
        Format::ALL.into_iter().find(|f| f.as_str() == text)
    }

    pub fn title(self) -> &'static str {
        match self {
            Format::Text => gezik_i18n::tx!("Text"),
            Format::Csv => "CSV",
            Format::Html => "HTML",
        }
    }
}

/// A column of an exported list: the name, the full path (CSV's last), or one the list shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Column {
    Name,
    Path,
    Shown(ColumnKey),
}

impl Column {
    pub fn title(self) -> &'static str {
        match self {
            Column::Name => "Name",
            Column::Path => "Path",
            Column::Shown(key) => key.title(),
        }
    }
}

/// The columns `format` writes when the list shows `shown` (in its order; Name is always first):
/// text one (the name, or the path with `full_paths`), CSV all of them and the path, HTML all.
pub fn columns(format: Format, shown: &[ColumnKey], full_paths: bool) -> Vec<Column> {
    match format {
        Format::Text => vec![if full_paths { Column::Path } else { Column::Name }],
        Format::Csv | Format::Html => {
            let mut out = vec![Column::Name];
            out.extend(shown.iter().map(|&key| Column::Shown(key)));
            if format == Format::Csv {
                out.push(Column::Path);
            }
            out
        }
    }
}

/// How a list is written.
#[derive(Debug, Clone)]
pub struct Sheet {
    pub format: Format,
    /// The column titles (CSV, HTML); text writes the first cell of each row and no titles.
    pub headers: Vec<String>,
    /// The columns HTML aligns right (sizes), as `headers`.
    pub right: Vec<bool>,
    /// CSV's separator: `,`, or the system's list separator (`;` on Turkish Windows).
    pub separator: char,
    /// CSV starts with a UTF-8 byte order mark (Excel on Windows reads the file as UTF-8 then).
    pub bom: bool,
    /// Text's and CSV's line end.
    pub eol: &'static str,
    /// HTML's title and heading (the folder or the search), and the line under it.
    pub title: String,
    pub note: String,
}

/// What comes before the rows.
pub fn begin(out: &mut dyn Write, sheet: &Sheet) -> io::Result<()> {
    match sheet.format {
        Format::Text => Ok(()),
        Format::Csv => {
            if sheet.bom {
                out.write_all("\u{feff}".as_bytes())?;
            }
            row(out, sheet, &sheet.headers)
        }
        Format::Html => {
            let title = html_escape(&sheet.title);
            write!(
                out,
                "<!DOCTYPE html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n<title>{title}</title>\n<style>\n{STYLE}</style>\n</head>\n<body>\n<h1>{title}</h1>\n<p>{}</p>\n<table>\n<thead><tr>",
                html_escape(&sheet.note)
            )?;
            for (i, header) in sheet.headers.iter().enumerate() {
                write!(out, "<th{}>{}</th>", right_class(sheet, i), html_escape(header))?;
            }
            out.write_all(b"</tr></thead>\n<tbody>\n")
        }
    }
}

/// One row.
pub fn row(out: &mut dyn Write, sheet: &Sheet, cells: &[String]) -> io::Result<()> {
    match sheet.format {
        Format::Text => {
            out.write_all(one_line(cells.first().map_or("", String::as_str)).as_bytes())?;
            out.write_all(sheet.eol.as_bytes())
        }
        Format::Csv => {
            for (i, cell) in cells.iter().enumerate() {
                if i > 0 {
                    write!(out, "{}", sheet.separator)?;
                }
                out.write_all(csv_field(cell, sheet.separator).as_bytes())?;
            }
            out.write_all(sheet.eol.as_bytes())
        }
        Format::Html => {
            out.write_all(b"<tr>")?;
            for (i, cell) in cells.iter().enumerate() {
                write!(out, "<td{}>{}</td>", right_class(sheet, i), html_escape(cell))?;
            }
            out.write_all(b"</tr>\n")
        }
    }
}

/// What comes after the rows.
pub fn end(out: &mut dyn Write, sheet: &Sheet) -> io::Result<()> {
    match sheet.format {
        Format::Text | Format::Csv => Ok(()),
        Format::Html => out.write_all(b"</tbody>\n</table>\n</body>\n</html>\n"),
    }
}

/// Light and dark by the reader's system; no script, no outside file.
const STYLE: &str = ":root { color-scheme: light dark; --fg: #1f2328; --bg: #ffffff; --muted: #59636e; --line: #d1d9e0; --head: #f6f8fa; }
@media (prefers-color-scheme: dark) { :root { --fg: #e6edf3; --bg: #0d1117; --muted: #9198a1; --line: #3d444d; --head: #151b23; } }
body { font: 14px system-ui, sans-serif; color: var(--fg); background: var(--bg); margin: 24px; }
h1 { font-size: 18px; margin: 0 0 4px; }
p { color: var(--muted); margin: 0 0 16px; }
table { border-collapse: collapse; }
th, td { padding: 4px 12px; border-bottom: 1px solid var(--line); text-align: left; white-space: nowrap; }
th { background: var(--head); }
.r { text-align: right; }
";

fn right_class(sheet: &Sheet, i: usize) -> &'static str {
    if sheet.right.get(i).copied().unwrap_or(false) { " class=\"r\"" } else { "" }
}

/// `text` as one CSV field (RFC 4180): quoted when it holds the separator, a quote or a line
/// break, its quotes doubled. One starting with `=`, `+`, `-`, `@`, a tab or CR gets a leading
/// `'`, so a spreadsheet never runs it as a formula (OWASP CSV injection).
pub fn csv_field(text: &str, separator: char) -> Cow<'_, str> {
    let text: Cow<'_, str> = if text.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        Cow::Owned(format!("'{text}"))
    } else {
        Cow::Borrowed(text)
    };
    if text.contains([separator, '"', '\n', '\r']) {
        Cow::Owned(format!("\"{}\"", text.replace('"', "\"\"")))
    } else {
        text
    }
}

/// `text` on one line of a text list: a line break in a name shows as `␤` (CR as `␍`).
fn one_line(text: &str) -> Cow<'_, str> {
    if text.contains(['\n', '\r']) {
        Cow::Owned(text.replace('\n', "␤").replace('\r', "␍"))
    } else {
        Cow::Borrowed(text)
    }
}

/// `text` safe inside HTML elements and attribute values.
pub fn html_escape(text: &str) -> Cow<'_, str> {
    if !text.contains(['&', '<', '>', '"', '\'']) {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len() + 16);
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    Cow::Owned(out)
}

/// A local date and time as CSV writes it: `2024-05-01T10:00:12` (ISO 8601, no zone).
pub fn iso_seconds(p: &DateParts) -> String {
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}", p.year, p.month, p.day, p.hour, p.minute, p.second)
}

/// `text` made fit for a file name on every system: `\ / : * ? " < > |` and control characters
/// become `_`, trailing dots and spaces go; `List` if nothing is left.
pub fn file_stem(text: &str) -> String {
    let stem: String =
        text.chars().map(|c| if c.is_control() || "\\/:*?\"<>|".contains(c) { '_' } else { c }).collect();
    let stem = stem.trim().trim_end_matches(['.', ' ']);
    if stem.is_empty() { "List".to_owned() } else { stem.to_owned() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sheet(format: Format) -> Sheet {
        Sheet {
            format,
            headers: vec!["Name".into(), "Size".into()],
            right: vec![false, true],
            separator: ',',
            bom: false,
            eol: "\n",
            title: "C:\\Work".into(),
            note: "2 items".into(),
        }
    }

    fn written(sheet: &Sheet, rows: &[[&str; 2]]) -> String {
        let mut out = Vec::new();
        begin(&mut out, sheet).unwrap();
        for cells in rows {
            row(&mut out, sheet, &cells.map(str::to_owned)).unwrap();
        }
        end(&mut out, sheet).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn csv_fields_are_quoted_only_when_they_must_be() {
        assert_eq!(csv_field("plain name.txt", ','), "plain name.txt");
        assert_eq!(csv_field(" spaces ", ','), " spaces ", "spaces stay as they are, unquoted");
        assert_eq!(csv_field("a,b", ','), "\"a,b\"");
        assert_eq!(csv_field("a,b", ';'), "a,b", "a comma is plain text with `;`");
        assert_eq!(csv_field("a;b", ';'), "\"a;b\"");
        assert_eq!(csv_field("say \"hi\"", ','), "\"say \"\"hi\"\"\"");
        assert_eq!(csv_field("two\nlines", ','), "\"two\nlines\"");
        assert_eq!(csv_field("cr\rhere", ','), "\"cr\rhere\"");
        assert_eq!(csv_field("", ','), "");
    }

    #[test]
    fn csv_fields_never_start_a_formula() {
        assert_eq!(csv_field("=cmd|' /C calc'!A0", ','), "'=cmd|' /C calc'!A0");
        assert_eq!(csv_field("+1.txt", ','), "'+1.txt");
        assert_eq!(csv_field("-rf", ','), "'-rf");
        assert_eq!(csv_field("@SUM(A1)", ','), "'@SUM(A1)");
        assert_eq!(csv_field("\tx", ','), "'\tx");
        assert_eq!(csv_field("\rx", ','), "\"'\rx\"", "guarded, then quoted for its CR");
        assert_eq!(csv_field("=a,b", ','), "\"'=a,b\"");
        assert_eq!(csv_field("a=b", ','), "a=b", "only at the start");
        assert_eq!(csv_field("12", ','), "12");
    }

    #[test]
    fn csv_has_titles_bom_separator_and_line_ends() {
        let mut csv = sheet(Format::Csv);
        assert_eq!(written(&csv, &[["a,b.txt", "12"]]), "Name,Size\n\"a,b.txt\",12\n");
        csv.separator = ';';
        csv.bom = true;
        csv.eol = "\r\n";
        let text = written(&csv, &[["a,b.txt", "12"], ["ş \"q\".txt", ""]]);
        assert_eq!(text, "\u{feff}Name;Size\r\na,b.txt;12\r\n\"ş \"\"q\"\".txt\";\r\n");
        assert!(text.as_bytes().starts_with(&[0xEF, 0xBB, 0xBF]));
    }

    #[test]
    fn text_is_one_cell_per_line() {
        let mut text = sheet(Format::Text);
        assert_eq!(written(&text, &[["a.txt", "1"], ["b.txt", "2"]]), "a.txt\nb.txt\n");
        text.eol = "\r\n";
        assert_eq!(written(&text, &[["a.txt", "1"]]), "a.txt\r\n");
        assert_eq!(written(&text, &[]), "", "an empty list: an empty file");
        assert_eq!(written(&text, &[["two\nlines\r.txt", ""]]), "two␤lines␍.txt\r\n", "a name never splits a line");
    }

    #[test]
    fn html_escapes_every_text_and_has_no_script() {
        let mut html = sheet(Format::Html);
        html.title = "C:\\<Work> & \"co\"".into();
        let text = written(&html, &[["<b>&'x\".txt", "1 KB"]]);
        assert!(text.starts_with("<!DOCTYPE html>"));
        assert!(text.contains("<title>C:\\&lt;Work&gt; &amp; &quot;co&quot;</title>"), "{text}");
        assert!(text.contains("<td>&lt;b&gt;&amp;&#39;x&quot;.txt</td><td class=\"r\">1 KB</td>"), "{text}");
        assert!(text.contains("<th class=\"r\">Size</th>"));
        assert!(text.contains("prefers-color-scheme: dark"));
        assert!(!text.contains("<script"));
        assert!(text.ends_with("</html>\n"));
        assert_eq!(html_escape("plain"), "plain");
    }

    #[test]
    fn formats_come_from_extensions() {
        assert_eq!(Format::from_path(Path::new("a/list.CSV")), Some(Format::Csv));
        assert_eq!(Format::from_path(Path::new("list.htm")), Some(Format::Html));
        assert_eq!(Format::from_path(Path::new("list.html")), Some(Format::Html));
        assert_eq!(Format::from_path(Path::new("list.txt")), Some(Format::Text));
        assert_eq!(Format::from_path(Path::new("list.doc")), None);
        assert_eq!(Format::from_path(Path::new("list")), None);
        for format in Format::ALL {
            assert_eq!(Format::parse(format.as_str()), Some(format));
        }
    }

    #[test]
    fn each_format_takes_its_columns() {
        let shown = [ColumnKey::Folder, ColumnKey::Size, ColumnKey::Modified];
        assert_eq!(columns(Format::Text, &shown, false), [Column::Name]);
        assert_eq!(columns(Format::Text, &shown, true), [Column::Path]);
        let csv = columns(Format::Csv, &shown, true);
        let titles: Vec<&str> = csv.iter().map(|c| c.title()).collect();
        assert_eq!(titles, ["Name", "Folder", "Size", "Modified", "Path"]);
        assert_eq!(columns(Format::Html, &shown, false).last(), Some(&Column::Shown(ColumnKey::Modified)));
    }

    #[test]
    fn dates_and_names() {
        let p = DateParts { year: 2024, month: 5, day: 1, hour: 10, minute: 0, second: 12 };
        assert_eq!(iso_seconds(&p), "2024-05-01T10:00:12");
        assert_eq!(file_stem("Search: *.pdf"), "Search_ _.pdf");
        assert_eq!(file_stem("Fotoğraflar"), "Fotoğraflar");
        assert_eq!(file_stem(" a. "), "a");
        assert_eq!(file_stem("..."), "List");
    }

    #[test]
    fn a_hundred_thousand_rows_are_written_within_a_second() {
        let csv = sheet(Format::Csv);
        let started = std::time::Instant::now();
        let mut out = io::BufWriter::with_capacity(64 * 1024, io::sink());
        begin(&mut out, &csv).unwrap();
        let mut cells = [String::new(), String::new()];
        for i in 0..100_000u32 {
            cells[0] = format!("file, number \"{i}\".txt");
            cells[1] = i.to_string();
            row(&mut out, &csv, &cells).unwrap();
        }
        end(&mut out, &csv).unwrap();
        out.flush().unwrap();
        assert!(started.elapsed() < std::time::Duration::from_secs(1), "{:?}", started.elapsed());
    }
}
