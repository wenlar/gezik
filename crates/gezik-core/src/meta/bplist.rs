//! The few binary property lists Finder's tag attributes hold (spec 12 §4.1): an array of
//! strings (`_kMDItemUserTags`), an integer (`kMDItemStarRating`) and a string
//! (`kMDItemFinderComment`). A list of any other shape reads as `None`, so it is never
//! written over.

/// What a list's top object is, of the kinds read here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Int(i64),
    Str(String),
    /// One level of strings.
    Strings(Vec<String>),
}

/// The most a list may decode to: Finder's attributes are a few hundred bytes; a made-up one
/// could otherwise point thousands of times at one large string.
const MAX_BYTES: usize = 64 * 1024;
const MAX_STRINGS: usize = 1000;

struct Doc<'a> {
    data: &'a [u8],
    offset_size: usize,
    count: usize,
    table: usize,
}

fn be(bytes: &[u8]) -> Option<u64> {
    (bytes.len() <= 8).then(|| bytes.iter().fold(0u64, |n, &b| (n << 8) | u64::from(b)))
}

impl Doc<'_> {
    fn offset(&self, index: usize) -> Option<usize> {
        if index >= self.count {
            return None;
        }
        let at = self.table.checked_add(index.checked_mul(self.offset_size)?)?;
        usize::try_from(be(self.data.get(at..at + self.offset_size)?)?).ok()
    }

    /// The object at `index`'s marker and where its body starts.
    fn object(&self, index: usize) -> Option<(u8, usize)> {
        let at = self.offset(index)?;
        Some((*self.data.get(at)?, at + 1))
    }

    /// A length in a marker's low nibble, or in the int object after it (`0xF`): the length
    /// and where the body starts.
    fn length(&self, marker: u8, at: usize) -> Option<(usize, usize)> {
        let low = marker & 0x0F;
        if low != 0x0F {
            return Some((usize::from(low), at));
        }
        let int = *self.data.get(at)?;
        if int >> 4 != 0x1 {
            return None;
        }
        let size = 1usize << (int & 0x0F);
        let n = usize::try_from(be(self.data.get(at + 1..at + 1 + size)?)?).ok()?;
        Some((n, at + 1 + size))
    }

    /// The string at `index`; `budget` is what the whole list may still decode (bytes), so many
    /// references to one big string cannot add up to more memory than there is.
    fn string(&self, index: usize, budget: &mut usize) -> Option<String> {
        let (marker, at) = self.object(index)?;
        let (len, at) = self.length(marker, at)?;
        *budget = budget.checked_sub(len.checked_mul(if marker >> 4 == 0x6 { 2 } else { 1 })?)?;
        match marker >> 4 {
            0x5 => {
                let bytes = self.data.get(at..at.checked_add(len)?)?;
                bytes.is_ascii().then(|| String::from_utf8_lossy(bytes).into_owned())
            }
            0x6 => {
                let bytes = self.data.get(at..at.checked_add(len.checked_mul(2)?)?)?;
                let units: Vec<u16> = bytes.as_chunks::<2>().0.iter().map(|pair| u16::from_be_bytes(*pair)).collect();
                String::from_utf16(&units).ok()
            }
            _ => None,
        }
    }
}

/// The top object of the list `data`, if it is one of [`Value`]'s kinds.
pub fn decode(data: &[u8]) -> Option<Value> {
    if data.len() < 8 + 32 || !data.starts_with(b"bplist00") {
        return None;
    }
    let trailer = &data[data.len() - 32..];
    let (offset_size, ref_size) = (usize::from(trailer[6]), usize::from(trailer[7]));
    if !(1..=8).contains(&offset_size) || !(1..=8).contains(&ref_size) {
        return None;
    }
    let count = usize::try_from(be(&trailer[8..16])?).ok()?;
    let top = usize::try_from(be(&trailer[16..24])?).ok()?;
    let table = usize::try_from(be(&trailer[24..32])?).ok()?;
    if table.checked_add(count.checked_mul(offset_size)?)? > data.len() - 32 {
        return None;
    }
    let doc = Doc { data, offset_size, count, table };
    let (marker, at) = doc.object(top)?;
    match marker >> 4 {
        0x1 => {
            let size = 1usize << (marker & 0x0F);
            let bytes = data.get(at..at + size)?;
            // 1, 2 and 4 bytes are unsigned, 8 signed (CFBinaryPlist).
            let n = be(bytes)?;
            Some(Value::Int(if size == 8 { n as i64 } else { i64::try_from(n).ok()? }))
        }
        0x5 | 0x6 => doc.string(top, &mut { MAX_BYTES }).map(Value::Str),
        0xA => {
            let (len, at) = doc.length(marker, at)?;
            if len > MAX_STRINGS {
                return None;
            }
            let mut budget = MAX_BYTES;
            let refs = data.get(at..at.checked_add(len.checked_mul(ref_size)?)?)?;
            refs.chunks(ref_size)
                .map(|r| doc.string(usize::try_from(be(r)?).ok()?, &mut budget))
                .collect::<Option<Vec<String>>>()
                .map(Value::Strings)
        }
        _ => None,
    }
}

/// Builds a list from objects (index 0 the top), as CFBinaryPlist lays it out.
fn assemble(objects: &[Vec<u8>]) -> Vec<u8> {
    let mut out = b"bplist00".to_vec();
    let mut offsets = Vec::with_capacity(objects.len());
    for object in objects {
        offsets.push(out.len());
        out.extend_from_slice(object);
    }
    let table = out.len();
    let offset_size = bytes_for(table as u64);
    for offset in offsets {
        out.extend_from_slice(&(offset as u64).to_be_bytes()[8 - offset_size..]);
    }
    let mut trailer = [0u8; 32];
    trailer[6] = offset_size as u8;
    trailer[7] = bytes_for(objects.len() as u64) as u8;
    trailer[8..16].copy_from_slice(&(objects.len() as u64).to_be_bytes());
    trailer[24..32].copy_from_slice(&(table as u64).to_be_bytes());
    out.extend_from_slice(&trailer);
    out
}

/// 1, 2, 4 or 8: the bytes an unsigned `n` needs.
fn bytes_for(n: u64) -> usize {
    match n {
        0..=0xFF => 1,
        0x100..=0xFFFF => 2,
        0x1_0000..=0xFFFF_FFFF => 4,
        _ => 8,
    }
}

/// A marker with a length: in its low nibble, or an int object after it.
fn marker(kind: u8, len: usize, out: &mut Vec<u8>) {
    if len < 0x0F {
        out.push((kind << 4) | len as u8);
    } else {
        out.push((kind << 4) | 0x0F);
        int_object(len as i64, out);
    }
}

fn int_object(n: i64, out: &mut Vec<u8>) {
    let size = if n < 0 { 8 } else { bytes_for(n as u64) };
    out.push(0x10 | size.trailing_zeros() as u8);
    out.extend_from_slice(&n.to_be_bytes()[8 - size..]);
}

/// ASCII as it is, anything else as UTF-16BE (as Finder writes it).
fn string_object(text: &str) -> Vec<u8> {
    let mut out = Vec::new();
    if text.is_ascii() {
        marker(0x5, text.len(), &mut out);
        out.extend_from_slice(text.as_bytes());
    } else {
        let units: Vec<u16> = text.encode_utf16().collect();
        marker(0x6, units.len(), &mut out);
        for unit in units {
            out.extend_from_slice(&unit.to_be_bytes());
        }
    }
    out
}

pub fn encode_strings(strings: &[String]) -> Vec<u8> {
    let ref_size = bytes_for(strings.len() as u64 + 1);
    let mut array = Vec::new();
    marker(0xA, strings.len(), &mut array);
    for i in 1..=strings.len() {
        array.extend_from_slice(&(i as u64).to_be_bytes()[8 - ref_size..]);
    }
    let mut objects = vec![array];
    objects.extend(strings.iter().map(|s| string_object(s)));
    assemble(&objects)
}

pub fn encode_int(n: i64) -> Vec<u8> {
    let mut object = Vec::new();
    int_object(n, &mut object);
    assemble(&[object])
}

pub fn encode_string(text: &str) -> Vec<u8> {
    assemble(&[string_object(text)])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `xattr -px com.apple.metadata:_kMDItemUserTags` of a file Finder tagged Red.
    const FINDER_RED: &[u8] = &[
        0x62, 0x70, 0x6C, 0x69, 0x73, 0x74, 0x30, 0x30, 0xA1, 0x01, 0x55, 0x52, 0x65, 0x64, 0x0A, 0x36, 0x08, 0x0A,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10,
    ];

    #[test]
    fn finders_red_tag_reads_and_writes_byte_for_byte() {
        assert_eq!(decode(FINDER_RED), Some(Value::Strings(vec!["Red\n6".into()])));
        assert_eq!(encode_strings(&["Red\n6".into()]), FINDER_RED);
    }

    #[test]
    fn utf16_long_and_many_strings_round_trip() {
        let strings: Vec<String> = vec![
            "Öncelikli\n2".into(),
            "Project X".into(),
            "a name longer than fourteen".into(),
            "🙂 smile".into(),
            "".into(),
        ];
        let data = encode_strings(&strings);
        assert_eq!(data[8], 0xA5);
        assert_eq!(decode(&data), Some(Value::Strings(strings)));
        let many: Vec<String> = (0..300).map(|i| format!("tag {i}")).collect();
        assert_eq!(decode(&encode_strings(&many)), Some(Value::Strings(many)), "two-byte refs and offsets");
        assert_eq!(decode(&encode_strings(&[])), Some(Value::Strings(Vec::new())));
    }

    #[test]
    fn ints_and_strings_round_trip() {
        for n in [0, 4, 255, 256, 70_000, -1] {
            assert_eq!(decode(&encode_int(n)), Some(Value::Int(n)));
        }
        assert_eq!(decode(&encode_string("Ready")), Some(Value::Str("Ready".into())));
        assert_eq!(decode(&encode_string("Hazır")), Some(Value::Str("Hazır".into())));
    }

    #[test]
    fn a_made_up_list_cannot_decode_to_much() {
        let too_many: Vec<String> = (0..=MAX_STRINGS).map(|i| i.to_string()).collect();
        assert_eq!(decode(&encode_strings(&too_many)), None);
        // 900 references to one 100-byte string: 90 KB from a list of 1 KB.
        let mut array = Vec::new();
        marker(0xA, 900, &mut array);
        array.extend(std::iter::repeat_n(1u8, 900));
        let data = assemble(&[array, string_object(&"x".repeat(100))]);
        assert!(data.len() < 2048);
        assert_eq!(decode(&data), None);
    }

    #[test]
    fn other_shapes_and_damage_read_as_none() {
        assert_eq!(decode(b"bplist00"), None);
        assert_eq!(decode(&FINDER_RED[..FINDER_RED.len() - 1]), None);
        let mut nested = FINDER_RED.to_vec();
        nested[10] = 0xA0; // the array holds an array
        assert_eq!(decode(&nested), None);
        let mut far = FINDER_RED.to_vec();
        far[17] = 0xF0; // an offset past the end
        assert_eq!(decode(&far), None);
        assert_eq!(decode(b"<?xml version=\"1.0\"?><plist></plist>                                 "), None);
        let mut dict = encode_int(1);
        dict[8] = 0xD0; // an empty dictionary
        assert_eq!(decode(&dict), None);
    }
}
