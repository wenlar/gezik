//! The metadata field registry (spec 13 §7.1): which fields there are, how a value shows and
//! compares, and which readers can fill which fields for an extension. Pure: the readers
//! themselves are in `gezik-batch` (`fields`), the consumers are the preview's panel and Info
//! (13d), columns, sorting and filters (13e), search and templates (13f).

use std::cmp::Ordering;

use crate::batch::date::DateParts;

/// A field: its index in [`FIELDS`].
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct FieldId(u8);

impl FieldId {
    pub fn def(self) -> &'static FieldDef {
        // Ids are only made from `FIELDS`' own indexes.
        &FIELDS[usize::from(self.0)]
    }

    /// Its place in the registry (menu ids, column numbers).
    pub fn number(self) -> i32 {
        i32::from(self.0)
    }

    /// The field at place `n` of the registry.
    pub fn from_number(n: i64) -> Option<FieldId> {
        u8::try_from(n).ok().filter(|n| usize::from(*n) < FIELDS_LEN).map(FieldId)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Group {
    Picture,
    Photo,
    Media,
    Music,
    Document,
}

impl Group {
    pub fn label(self) -> &'static str {
        match self {
            Group::Picture => "Picture",
            Group::Photo => "Photo",
            Group::Media => "Media",
            Group::Music => "Music",
            Group::Document => "Document",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ValueKind {
    Text,
    Int,
    Real,
    /// Milliseconds.
    Duration,
    Date,
    /// Latitude, longitude.
    Geo,
}

pub struct FieldDef {
    /// `artist`, `duration`: settings, filter words, templates.
    pub key: &'static str,
    /// `Artist`: column header, panel, pickers.
    pub label: &'static str,
    pub group: Group,
    pub kind: ValueKind,
    /// A new column's width (logical px).
    pub width: u16,
    /// Grouping by it makes sense (text fields and years; spec 13 decision 25).
    pub groupable: bool,
}

impl FieldDef {
    /// Numbers and durations are right-aligned in columns.
    pub fn right_align(&self) -> bool {
        !matches!(self.kind, ValueKind::Text | ValueKind::Date)
    }
}

macro_rules! fields {
    ($($id:ident $key:literal $label:literal $group:ident $kind:ident $width:literal $groupable:literal;)*) => {
        #[allow(non_camel_case_types, clippy::upper_case_acronyms)]
        enum Index { $($id),* }
        $(pub const $id: FieldId = FieldId(Index::$id as u8);)*
        pub static FIELDS: &[FieldDef] = &[$(FieldDef {
            key: $key,
            label: $label,
            group: Group::$group,
            kind: ValueKind::$kind,
            width: $width,
            groupable: $groupable,
        }),*];
    };
}

// The order is the panel's: by group, then as a reader of that kind lists them.
fields! {
    WIDTH "width" "Width" Picture Int 70 false;
    HEIGHT "height" "Height" Picture Int 70 false;
    DIMENSIONS "dimensions" "Dimensions" Picture Int 110 false;
    TAKEN "taken" "Date Taken" Photo Date 140 true;
    CAMERA "camera" "Camera" Photo Text 140 true;
    LENS "lens" "Lens" Photo Text 140 true;
    EXPOSURE "exposure" "Exposure" Photo Real 80 false;
    APERTURE "aperture" "Aperture" Photo Real 70 false;
    ISO "iso" "ISO" Photo Int 60 false;
    FOCAL_LENGTH "focal-length" "Focal Length" Photo Real 90 false;
    GPS "gps" "Location" Photo Geo 170 false;
    DURATION "duration" "Duration" Media Duration 70 false;
    VIDEO_CODEC "video-codec" "Video Codec" Media Text 90 true;
    AUDIO_CODEC "audio-codec" "Audio Codec" Media Text 90 true;
    FRAME_RATE "frame-rate" "Frame Rate" Media Real 80 false;
    BITRATE "bitrate" "Bit Rate" Media Int 80 false;
    SAMPLE_RATE "sample-rate" "Sample Rate" Media Int 90 false;
    CHANNELS "channels" "Channels" Media Int 70 false;
    TITLE "title" "Title" Music Text 180 true;
    ARTIST "artist" "Artist" Music Text 150 true;
    ALBUM "album" "Album" Music Text 150 true;
    ALBUM_ARTIST "album-artist" "Album Artist" Music Text 150 true;
    GENRE "genre" "Genre" Music Text 100 true;
    COMPOSER "composer" "Composer" Music Text 150 true;
    TRACK "track" "Track" Music Int 50 false;
    DISC "disc" "Disc" Music Int 50 false;
    YEAR "year" "Year" Music Int 60 true;
    AUTHOR "author" "Author" Document Text 150 true;
    SUBJECT "subject" "Subject" Document Text 150 true;
    PAGES "pages" "Pages" Document Int 60 false;
    WORDS "words" "Words" Document Int 70 false;
}

/// Every field, in the registry's order.
pub fn all() -> impl Iterator<Item = FieldId> {
    (0..FIELDS_LEN as u8).map(FieldId)
}

/// The field with this key.
pub fn by_key(key: &str) -> Option<FieldId> {
    FIELDS.iter().position(|f| f.key == key).and_then(|i| u8::try_from(i).ok()).map(FieldId)
}

/// Which fields a consumer needs.
#[derive(Clone, Copy, Default, PartialEq, Eq, Hash, Debug)]
pub struct FieldSet(u64);

impl FieldSet {
    pub const NONE: FieldSet = FieldSet(0);
    pub const ALL: FieldSet = FieldSet(u64::MAX >> (64 - FIELDS_LEN));

    pub const fn of(ids: &[FieldId]) -> FieldSet {
        let mut bits = 0;
        let mut i = 0;
        while i < ids.len() {
            bits |= 1 << ids[i].0;
            i += 1;
        }
        FieldSet(bits)
    }

    pub fn has(self, id: FieldId) -> bool {
        self.0 & (1 << id.0) != 0
    }

    pub fn meets(self, other: FieldSet) -> bool {
        self.0 & other.0 != 0
    }

    #[must_use]
    pub const fn without(self, other: FieldSet) -> FieldSet {
        FieldSet(self.0 & !other.0)
    }

    #[must_use]
    pub const fn with(self, other: FieldSet) -> FieldSet {
        FieldSet(self.0 | other.0)
    }

    /// The fields in both.
    #[must_use]
    pub const fn and(self, other: FieldSet) -> FieldSet {
        FieldSet(self.0 & other.0)
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Whether every field of `other` is in it.
    pub fn covers(self, other: FieldSet) -> bool {
        other.0 & !self.0 == 0
    }
}

const FIELDS_LEN: usize = Index::WORDS as usize + 1;
const _: () = assert!(FIELDS_LEN <= 64, "a FieldSet is a u64");

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Text(Box<str>),
    Int(i64),
    Real(f64),
    /// Milliseconds.
    Duration(u64),
    Date(DateParts),
    Geo(f64, f64),
}

/// One file's values, sorted by field; a missing field is absent.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Fields(Box<[(FieldId, Value)]>);

impl Fields {
    pub fn new(mut values: Vec<(FieldId, Value)>) -> Fields {
        values.sort_by_key(|(id, _)| *id);
        values.dedup_by_key(|(id, _)| *id);
        Fields(values.into_boxed_slice())
    }

    pub fn get(&self, id: FieldId) -> Option<&Value> {
        self.0.binary_search_by_key(&id, |(i, _)| *i).ok().and_then(|i| self.0.get(i)).map(|(_, v)| v)
    }

    pub fn iter(&self) -> impl Iterator<Item = (FieldId, &Value)> {
        self.0.iter().map(|(id, v)| (*id, v))
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The one display form of a field (panel, column): `4032 × 3024`, `1/250 s`, `3:25`.
    pub fn text(&self, id: FieldId) -> Option<String> {
        let value = self.get(id)?;
        if id == DIMENSIONS
            && let (Some(Value::Int(w)), Some(Value::Int(h))) = (self.get(WIDTH), self.get(HEIGHT))
        {
            return Some(format!("{w} × {h}"));
        }
        Some(text(id, value))
    }
}

/// A group header's text for `value` (spec 13 §8.2): the value as shown, a date by its year.
pub fn group_text(id: FieldId, value: &Value) -> String {
    match value {
        Value::Date(d) => d.year.to_string(),
        _ => text(id, value),
    }
}

/// `value` as `id` shows it, without the other fields (dimensions show as a pixel count).
pub fn text(id: FieldId, value: &Value) -> String {
    match value {
        Value::Text(text) => text.to_string(),
        Value::Int(n) => match id {
            WIDTH | HEIGHT => format!("{n} px"),
            BITRATE => format!("{n} kb/s"),
            SAMPLE_RATE => format!("{} Hz", crate::group_digits(n.unsigned_abs())),
            DIMENSIONS | PAGES | WORDS => crate::group_digits(n.unsigned_abs()),
            _ => n.to_string(),
        },
        Value::Real(x) => match (id, gezik_i18n::number()) {
            (EXPOSURE, _) if *x > 0.0 && *x < 1.0 => format!("1/{} s", (1.0 / x).round()),
            (EXPOSURE, number) => format!("{} s", trim(*x, 1, number)),
            (APERTURE, number) => format!("f/{}", trim(*x, 1, number)),
            (FOCAL_LENGTH, number) => format!("{} mm", trim(*x, 1, number)),
            (FRAME_RATE, number) => format!("{} fps", trim(*x, 2, number)),
            (_, number) => trim(*x, 2, number),
        },
        Value::Duration(ms) => duration_text(*ms),
        Value::Date(d) => {
            format!("{:04}-{:02}-{:02} {:02}:{:02}", d.year, d.month, d.day, d.hour, d.minute)
        }
        Value::Geo(lat, lon) => format!(
            "{:.4}° {}, {:.4}° {}",
            lat.abs(),
            if *lat < 0.0 { "S" } else { "N" },
            lon.abs(),
            if *lon < 0.0 { "W" } else { "E" }
        ),
    }
}

/// `x` with at most `places` decimals, no trailing zeros, with `number`'s decimal separator.
fn trim(x: f64, places: usize, number: gezik_i18n::Number) -> String {
    let mut text = String::new();
    number.write_decimal(&mut text, x, places);
    if places > 0 {
        let keep = text.trim_end_matches('0').trim_end_matches(number.decimal).len();
        text.truncate(keep);
    }
    text
}

/// `3:25`, `1:02:03`.
pub fn duration_text(ms: u64) -> String {
    let s = ms / 1000;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

/// `value` in a name template (spec 13 §9.2), `arg` its format: a number zero-padded to `arg`
/// digits (`{track:02}`), a date in the `arg` format (`{taken:%Y}`), a real number with `arg`
/// decimals (`{aperture:1}`), a duration as `3m25s` (no `:` in names). Not yet name-safe:
/// the template cleans it.
pub fn name_text(id: FieldId, value: &Value, arg: Option<&str>) -> String {
    let digits = arg.and_then(|a| a.trim().parse::<usize>().ok());
    match value {
        Value::Int(n) => match digits {
            Some(width) if *n >= 0 => format!("{n:0width$}"),
            _ => n.to_string(),
        },
        Value::Real(x) => match digits {
            Some(places) => format!("{x:.places$}"),
            None if id == EXPOSURE && *x > 0.0 && *x < 1.0 => format!("1-{}", (1.0 / x).round()),
            // A name does not change with the interface language.
            None => trim(*x, 2, gezik_i18n::Number::ENGLISH),
        },
        Value::Duration(ms) => {
            let s = ms / 1000;
            match (s / 3600, s / 60 % 60, s % 60) {
                (0, 0, s) => format!("{s}s"),
                (0, m, s) => format!("{m}m{s:02}s"),
                (h, m, s) => format!("{h}h{m:02}m{s:02}s"),
            }
        }
        Value::Date(d) => crate::batch::date::format(arg.unwrap_or("%Y-%m-%d"), d),
        _ => text(id, value),
    }
}

/// Sorting order of two values of a field; a missing value comes last, text in the names' order
/// (natural, Turkish alphabet, case ignored).
pub fn compare(a: Option<&Value>, b: Option<&Value>) -> Ordering {
    match (a, b) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(a), Some(b)) => match (a, b) {
            (Value::Text(a), Value::Text(b)) => crate::sort::key_cmp(a, b),
            (Value::Int(a), Value::Int(b)) => a.cmp(b),
            (Value::Real(a), Value::Real(b)) => a.total_cmp(b),
            (Value::Duration(a), Value::Duration(b)) => a.cmp(b),
            (Value::Date(a), Value::Date(b)) => date_key(a).cmp(&date_key(b)),
            (Value::Geo(a, b1), Value::Geo(c, d)) => a.total_cmp(c).then(b1.total_cmp(d)),
            _ => Ordering::Equal,
        },
    }
}

fn date_key(d: &DateParts) -> (i32, u8, u8, u8, u8, u8) {
    (d.year, d.month, d.day, d.hour, d.minute, d.second)
}

/// A value typed by the user (filters, searches): `3m`, `1:30`, `2024-05`, `50 mm`, `f/2.8`,
/// `1/250`. A date's missing parts are 0 (`2024` sorts before every day of 2024).
pub fn parse_value(kind: ValueKind, text: &str) -> Result<Value, String> {
    let text = text.trim();
    let bad = || Err(format!("\"{text}\" is not a {}", kind_name(kind)));
    match kind {
        ValueKind::Text if text.is_empty() => bad(),
        ValueKind::Text => Ok(Value::Text(text.into())),
        ValueKind::Int => {
            let digits: String = number_part(text).chars().filter(|c| *c != ',').collect();
            digits.parse().map(Value::Int).or_else(|_| bad())
        }
        ValueKind::Real => real(text).map(Value::Real).map_or_else(bad, Ok),
        ValueKind::Duration => duration(text).map(Value::Duration).map_or_else(bad, Ok),
        ValueKind::Date => date(text).map(Value::Date).map_or_else(bad, Ok),
        ValueKind::Geo => {
            let Some((lat, lon)) = text.split_once(',') else { return bad() };
            match (lat.trim().parse::<f64>(), lon.trim().parse::<f64>()) {
                (Ok(lat), Ok(lon)) if lat.abs() <= 90.0 && lon.abs() <= 180.0 => Ok(Value::Geo(lat, lon)),
                _ => bad(),
            }
        }
    }
}

fn kind_name(kind: ValueKind) -> &'static str {
    match kind {
        ValueKind::Text => "text",
        ValueKind::Int => "number",
        ValueKind::Real => "number",
        ValueKind::Duration => "duration (3m, 1:30)",
        ValueKind::Date => "date (2024, 2024-05, 2024-05-01)",
        ValueKind::Geo => "location (41.0, 28.9)",
    }
}

/// The leading number of `50 mm`, `320 kb/s`, `1,204`.
fn number_part(text: &str) -> &str {
    let end = text
        .char_indices()
        .find(|&(i, c)| !(c.is_ascii_digit() || c == '.' || c == ',' || (i == 0 && (c == '-' || c == '+'))))
        .map_or(text.len(), |(i, _)| i);
    text.get(..end).unwrap_or_default()
}

fn real(text: &str) -> Option<f64> {
    let text = text.strip_prefix("f/").or_else(|| text.strip_prefix("F/")).unwrap_or(text);
    if let Some((n, d)) = text.split_once('/') {
        let (n, d): (f64, f64) = (number_part(n.trim()).parse().ok()?, number_part(d.trim()).parse().ok()?);
        return (d != 0.0).then(|| n / d);
    }
    number_part(text).replace(',', "").parse().ok()
}

/// `1:30`, `1:02:03`, `90s`, `3m`, `1h30m`, `2.5m`; a bare number is seconds.
fn duration(text: &str) -> Option<u64> {
    let seconds = if text.contains(':') {
        let parts: Vec<&str> = text.split(':').collect();
        if parts.len() > 3 {
            return None;
        }
        let mut total = 0.0;
        for part in parts {
            total = total * 60.0 + part.trim().parse::<f64>().ok().filter(|x| *x >= 0.0)?;
        }
        total
    } else {
        let mut total = 0.0;
        let mut rest = text.trim();
        if rest.is_empty() {
            return None;
        }
        while !rest.is_empty() {
            let number = number_part(rest);
            let value: f64 = number.parse().ok().filter(|x: &f64| *x >= 0.0)?;
            rest = rest.get(number.len()..)?.trim_start();
            let unit_len = rest.find(|c: char| !c.is_ascii_alphabetic()).unwrap_or(rest.len());
            let unit = rest.get(..unit_len)?;
            total += value
                * match unit.to_ascii_lowercase().as_str() {
                    "" | "s" | "sec" => 1.0,
                    "m" | "min" => 60.0,
                    "h" | "hr" => 3600.0,
                    _ => return None,
                };
            rest = rest.get(unit_len..)?.trim_start();
        }
        total
    };
    seconds.is_finite().then(|| (seconds * 1000.0).round() as u64)
}

/// `2024`, `2024-05`, `2024-05-01`, `2024:05:01 10:30` (EXIF's form).
fn date(text: &str) -> Option<DateParts> {
    let (day, time) = text.split_once([' ', 'T']).unwrap_or((text, ""));
    let mut d = day.split(['-', ':', '.', '/']);
    let year: i32 = d.next()?.parse().ok().filter(|y| (1..=9999).contains(y))?;
    let mut part =
        |max: u8| -> Option<u8> { d.next().map_or(Some(0), |p| p.parse().ok().filter(|v| (1..=max).contains(v))) };
    let (month, day) = (part(12)?, part(31)?);
    let mut t = time.split(':').filter(|p| !p.is_empty());
    let mut clock = |max: u8| -> Option<u8> { t.next().map_or(Some(0), |p| p.parse().ok().filter(|v| *v <= max)) };
    let (hour, minute, second) = (clock(23)?, clock(59)?, clock(59)?);
    Some(DateParts { year, month, day, hour, minute, second })
}

/// The format readers (`gezik-batch` `fields`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Reader {
    Image,
    Exif,
    Audio,
    Video,
    Document,
    Pdf,
}

/// Which reader can fill what, by extension: a description, no I/O.
pub struct ReaderDef {
    pub reader: Reader,
    pub exts: &'static [&'static str],
    pub provides: FieldSet,
}

const PICTURE: FieldSet = FieldSet::of(&[WIDTH, HEIGHT, DIMENSIONS]);
const MEDIA: FieldSet = FieldSet::of(&[DURATION, VIDEO_CODEC, AUDIO_CODEC, FRAME_RATE, BITRATE, SAMPLE_RATE, CHANNELS]);
const MUSIC: FieldSet = FieldSet::of(&[TITLE, ARTIST, ALBUM, ALBUM_ARTIST, GENRE, COMPOSER, TRACK, DISC, YEAR]);

pub static READERS: [ReaderDef; 6] = [
    ReaderDef {
        reader: Reader::Image,
        exts: &["png", "jpg", "jpeg", "gif", "bmp", "webp", "tif", "tiff", "ico", "heic", "heif", "avif"],
        provides: PICTURE,
    },
    ReaderDef {
        reader: Reader::Exif,
        exts: &["jpg", "jpeg", "tif", "tiff", "heic", "heif", "webp", "png", "dng", "cr2", "nef", "arw"],
        provides: FieldSet::of(&[TAKEN, CAMERA, LENS, EXPOSURE, APERTURE, ISO, FOCAL_LENGTH, GPS]),
    },
    ReaderDef {
        reader: Reader::Audio,
        exts: &["mp3", "flac", "ogg", "oga", "opus", "m4a", "aac", "wav", "aiff", "aif"],
        provides: MEDIA.with(MUSIC),
    },
    ReaderDef {
        reader: Reader::Video,
        exts: &["mp4", "m4v", "mov", "mkv", "webm", "avi"],
        provides: PICTURE.with(MEDIA).with(MUSIC),
    },
    ReaderDef {
        reader: Reader::Document,
        exts: &["docx", "xlsx", "pptx", "odt", "ods", "odp"],
        provides: FieldSet::of(&[TITLE, AUTHOR, SUBJECT, PAGES, WORDS]),
    },
    ReaderDef { reader: Reader::Pdf, exts: &["pdf"], provides: FieldSet::of(&[TITLE, AUTHOR, SUBJECT, PAGES]) },
];

/// The fields some reader of extension `ext` (lower case) can give: none, nothing is read.
pub fn provided(ext: &str) -> FieldSet {
    READERS.iter().filter(|r| r.exts.contains(&ext)).fold(FieldSet::NONE, |set, r| set.with(r.provides))
}

/// The readers to run for a file with extension `ext` (lower case) when `want` is needed:
/// none when no reader of that extension gives any of them (nothing is read then).
pub fn readers_for(ext: &str, want: FieldSet) -> impl Iterator<Item = Reader> {
    READERS.iter().filter(move |r| r.provides.meets(want) && r.exts.contains(&ext)).map(|r| r.reader)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_unique_and_found() {
        assert_eq!(FIELDS.len(), FIELDS_LEN);
        for (i, field) in FIELDS.iter().enumerate() {
            assert_eq!(by_key(field.key).map(|id| usize::from(id.0)), Some(i), "{}", field.key);
            assert!(FIELDS.iter().filter(|f| f.label == field.label).count() == 1);
        }
        assert_eq!(by_key("artist"), Some(ARTIST));
        assert_eq!(ARTIST.def().label, "Artist");
        assert!(by_key("Artist").is_none());
        assert!(FieldSet::ALL.has(WORDS) && !FieldSet::NONE.has(WIDTH));
        assert!(DURATION.def().right_align() && !TITLE.def().right_align());
    }

    #[test]
    fn values_show_in_one_form() {
        let fields = Fields::new(vec![
            (HEIGHT, Value::Int(3024)),
            (WIDTH, Value::Int(4032)),
            (DIMENSIONS, Value::Int(4032 * 3024)),
            (EXPOSURE, Value::Real(0.004)),
            (APERTURE, Value::Real(2.8)),
            (FOCAL_LENGTH, Value::Real(50.0)),
            (DURATION, Value::Duration(205_400)),
            (BITRATE, Value::Int(320)),
            (SAMPLE_RATE, Value::Int(44_100)),
            (FRAME_RATE, Value::Real(29.97)),
            (GPS, Value::Geo(41.0082, -28.97841)),
            (TITLE, Value::Text("Ağıt".into())),
            (TAKEN, Value::Date(DateParts { year: 2024, month: 7, day: 1, hour: 9, minute: 5, second: 0 })),
        ]);
        let shown = |id| fields.text(id).unwrap();
        assert_eq!(shown(DIMENSIONS), "4032 × 3024");
        assert_eq!(shown(WIDTH), "4032 px");
        assert_eq!(shown(EXPOSURE), "1/250 s");
        assert_eq!(shown(APERTURE), "f/2.8");
        assert_eq!(shown(FOCAL_LENGTH), "50 mm");
        assert_eq!(shown(DURATION), "3:25");
        assert_eq!(shown(BITRATE), "320 kb/s");
        assert_eq!(shown(SAMPLE_RATE), "44,100 Hz");
        assert_eq!(shown(FRAME_RATE), "29.97 fps");
        assert_eq!(shown(GPS), "41.0082° N, 28.9784° W");
        assert_eq!(shown(TITLE), "Ağıt");
        assert_eq!(shown(TAKEN), "2024-07-01 09:05");
        assert_eq!(duration_text(3_723_000), "1:02:03");
        assert_eq!(text(EXPOSURE, &Value::Real(2.0)), "2 s");
        assert!(fields.text(ARTIST).is_none());
        assert_eq!(fields.iter().next().map(|(id, _)| id), Some(WIDTH), "sorted by field");
    }

    #[test]
    fn decimals_take_the_language_separator() {
        let tr = gezik_i18n::Number::builtin("tr").unwrap();
        assert_eq!(trim(2.8, 1, tr), "2,8");
        assert_eq!(trim(3.0, 2, tr), "3");
        assert_eq!(trim(29.97, 2, tr), "29,97");
        assert_eq!(trim(100.0, 0, tr), "100");
        assert_eq!(trim(29.97, 2, gezik_i18n::Number::ENGLISH), "29.97");
    }

    #[test]
    fn values_in_names_take_their_format() {
        assert_eq!(name_text(TRACK, &Value::Int(7), Some("02")), "07");
        assert_eq!(name_text(TRACK, &Value::Int(7), None), "7");
        assert_eq!(name_text(WIDTH, &Value::Int(4032), None), "4032", "no unit, no grouping");
        assert_eq!(name_text(APERTURE, &Value::Real(2.8), Some("1")), "2.8");
        assert_eq!(name_text(APERTURE, &Value::Real(2.0), None), "2");
        assert_eq!(name_text(EXPOSURE, &Value::Real(0.004), None), "1-250");
        assert_eq!(name_text(DURATION, &Value::Duration(205_400), None), "3m25s");
        assert_eq!(name_text(DURATION, &Value::Duration(3_723_000), None), "1h02m03s");
        assert_eq!(name_text(DURATION, &Value::Duration(9_000), None), "9s");
        let day = Value::Date(DateParts { year: 2024, month: 7, day: 1, ..DateParts::default() });
        assert_eq!(name_text(TAKEN, &day, Some("%Y")), "2024");
        assert_eq!(name_text(TAKEN, &day, None), "2024-07-01");
        assert_eq!(name_text(ARTIST, &Value::Text("Tarkan".into()), None), "Tarkan");
    }

    #[test]
    fn missing_values_sort_last_and_text_ignores_case() {
        let (a, b) = (Value::Text("abba".into()), Value::Text("Beatles".into()));
        assert_eq!(compare(Some(&a), Some(&b)), Ordering::Less);
        assert_eq!(compare(None, Some(&a)), Ordering::Greater);
        assert_eq!(compare(Some(&a), None), Ordering::Less);
        assert_eq!(compare(None, None), Ordering::Equal);
        assert_eq!(compare(Some(&Value::Int(2)), Some(&Value::Int(10))), Ordering::Less);
        assert_eq!(compare(Some(&Value::Real(2.8)), Some(&Value::Real(1.8))), Ordering::Greater);
        assert_eq!(compare(Some(&Value::Duration(1)), Some(&Value::Duration(1))), Ordering::Equal);
        let day = |d| Value::Date(DateParts { year: 2024, month: 5, day: d, ..DateParts::default() });
        assert_eq!(compare(Some(&day(1)), Some(&day(2))), Ordering::Less);
        assert_eq!(compare(Some(&Value::Geo(1.0, 2.0)), Some(&Value::Geo(1.0, 3.0))), Ordering::Less);
    }

    #[test]
    fn typed_values_parse() {
        use ValueKind::*;
        assert_eq!(parse_value(Duration, "3m"), Ok(Value::Duration(180_000)));
        assert_eq!(parse_value(Duration, "1:30"), Ok(Value::Duration(90_000)));
        assert_eq!(parse_value(Duration, "1:02:03"), Ok(Value::Duration(3_723_000)));
        assert_eq!(parse_value(Duration, "1h30m"), Ok(Value::Duration(5_400_000)));
        assert_eq!(parse_value(Duration, "2.5m"), Ok(Value::Duration(150_000)));
        assert_eq!(parse_value(Duration, "45"), Ok(Value::Duration(45_000)));
        assert!(parse_value(Duration, "3x").is_err() && parse_value(Duration, "").is_err());
        assert!(parse_value(Duration, "1:2:3:4").is_err() && parse_value(Duration, "-1m").is_err());
        let date = |y, m, d| Value::Date(DateParts { year: y, month: m, day: d, ..DateParts::default() });
        assert_eq!(parse_value(Date, "2024"), Ok(date(2024, 0, 0)));
        assert_eq!(parse_value(Date, "2024-05"), Ok(date(2024, 5, 0)));
        assert_eq!(parse_value(Date, "2024-05-01"), Ok(date(2024, 5, 1)));
        assert_eq!(
            parse_value(Date, "2024:05:01 10:30"),
            Ok(Value::Date(DateParts { year: 2024, month: 5, day: 1, hour: 10, minute: 30, second: 0 }))
        );
        assert!(parse_value(Date, "2024-13").is_err() && parse_value(Date, "May").is_err());
        assert_eq!(parse_value(Real, "50 mm"), Ok(Value::Real(50.0)));
        assert_eq!(parse_value(Real, "f/2.8"), Ok(Value::Real(2.8)));
        assert_eq!(parse_value(Real, "1/250"), Ok(Value::Real(0.004)));
        assert!(parse_value(Real, "1/0").is_err() && parse_value(Real, "fast").is_err());
        assert_eq!(parse_value(Int, "320 kb/s"), Ok(Value::Int(320)));
        assert_eq!(parse_value(Int, "1,204"), Ok(Value::Int(1204)));
        assert!(parse_value(Int, "many").is_err());
        assert_eq!(parse_value(Text, " Queen "), Ok(Value::Text("Queen".into())));
        assert_eq!(parse_value(Geo, "41.0, 28.9"), Ok(Value::Geo(41.0, 28.9)));
        assert!(parse_value(Geo, "91, 0").is_err());
    }

    #[test]
    fn only_readers_that_give_a_wanted_field_run() {
        let run = |ext, want| readers_for(ext, want).collect::<Vec<_>>();
        assert_eq!(run("jpg", FieldSet::ALL), [Reader::Image, Reader::Exif]);
        assert_eq!(run("jpg", FieldSet::of(&[CAMERA])), [Reader::Exif]);
        assert!(run("jpg", FieldSet::of(&[ARTIST])).is_empty());
        assert!(run("mp3", FieldSet::of(&[WIDTH])).is_empty());
        assert!(run("txt", FieldSet::ALL).is_empty());
        assert_eq!(run("mp3", FieldSet::of(&[DURATION])), [Reader::Audio]);
        assert_eq!(run("mkv", FieldSet::of(&[TITLE])), [Reader::Video]);
        assert_eq!(run("docx", FieldSet::of(&[TITLE, PAGES])), [Reader::Document]);
        assert_eq!(run("pdf", FieldSet::ALL), [Reader::Pdf]);
        assert!(provided("txt").is_empty() && provided("mp3").has(ARTIST) && !provided("mp3").has(WIDTH));
        assert!(FieldSet::ALL.covers(provided("jpg")) && !provided("jpg").covers(FieldSet::ALL));
        assert_eq!(FieldSet::of(&[ARTIST, WIDTH]).and(provided("mp3")), FieldSet::of(&[ARTIST]));
    }
}
