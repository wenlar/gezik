//! The secret code (spec 17 §5.1): 25 Crockford base32 characters, 24 random (120 bits) and a
//! check character, shown in groups of five (`7KQ2M-X9D4R-…`); and the keys HKDF gives from it.

use hkdf::Hkdf;
use sha2::Sha256;

/// Crockford's base32: no I, L, O or U (read as 1, 1, 0 and refused).
const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
const LEN: usize = 25;

/// What a code that does not read is told.
pub const TYPO: &str = gezik_i18n::tx!("This code has a typo");

/// A new code, as it is shown.
pub fn new_code() -> std::io::Result<String> {
    let mut random = [0u8; 15];
    getrandom::fill(&mut random).map_err(std::io::Error::other)?;
    // 120 bits make 24 symbols of five.
    let bits = random.iter().fold(0u128, |all, &byte| (all << 8) | u128::from(byte));
    let mut symbols: Vec<u8> = (0..LEN - 1).rev().map(|i| ((bits >> (i * 5)) & 31) as u8).collect();
    symbols.push(check(&symbols));
    Ok(display(&symbols.iter().map(|&s| ALPHABET[s as usize] as char).collect::<String>()))
}

/// The code `text` (any case, spaces and dashes anywhere, O for 0, I and L for 1) in its plain
/// form (25 characters, what [`Keys::derive`] takes), or [`TYPO`].
pub fn parse_code(text: &str) -> Result<String, &'static str> {
    let mut symbols = Vec::with_capacity(LEN);
    for c in text.chars().filter(|c| !c.is_whitespace() && *c != '-') {
        let c = match c.to_ascii_uppercase() {
            'O' => '0',
            'I' | 'L' => '1',
            c => c,
        };
        symbols.push(ALPHABET.iter().position(|&a| a as char == c).ok_or(TYPO)? as u8);
    }
    if symbols.len() != LEN || check(&symbols[..LEN - 1]) != symbols[LEN - 1] {
        return Err(TYPO);
    }
    Ok(symbols.iter().map(|&s| ALPHABET[s as usize] as char).collect())
}

/// A plain code in groups of five joined by dashes.
pub fn display(code: &str) -> String {
    code.as_bytes().chunks(5).map(|group| String::from_utf8_lossy(group)).collect::<Vec<_>>().join("-")
}

/// CRC-5 (x⁵ + x² + 1, the USB one, register starting at all ones, not reflected) over the
/// symbols' bits, most significant first: any one mistyped character is caught, as an error
/// within five bits always is.
fn check(symbols: &[u8]) -> u8 {
    let mut register = 0x1F;
    for symbol in symbols {
        for bit in (0..5).rev() {
            let feedback = ((register >> 4) ^ (symbol >> bit)) & 1;
            register = (register << 1) & 0x1F;
            if feedback == 1 {
                register ^= 0x05;
            }
        }
    }
    register
}

/// What a code gives (HKDF-SHA256 over its plain form, no salt; one `info` label each): the
/// box's name on the server, the proof of knowing the code, the bundle's key. The code's 120
/// random bits need no slow KDF.
pub struct Keys {
    pub id: [u8; 16],
    pub auth: [u8; 32],
    pub key: [u8; 32],
}

impl Keys {
    /// The keys of `code`, a plain code from [`parse_code`].
    pub fn derive(code: &str) -> Keys {
        let hkdf = Hkdf::<Sha256>::new(None, code.as_bytes());
        let mut keys = Keys { id: [0; 16], auth: [0; 32], key: [0; 32] };
        // Fails only for an output longer than 255 hashes.
        let expand = |info: &str, out: &mut [u8]| hkdf.expand(info.as_bytes(), out).expect("short output");
        expand("gezik-sync id v1", &mut keys.id);
        expand("gezik-sync auth v1", &mut keys.auth);
        expand("gezik-sync key v1", &mut keys.key);
        keys
    }

    /// The id as the server's addresses have it (32 lowercase hex digits).
    pub fn id_hex(&self) -> String {
        self.id.iter().map(|byte| format!("{byte:02x}")).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_code_reads_back() {
        let shown = new_code().unwrap();
        assert_eq!(shown.len(), 29, "{shown}");
        assert_eq!(shown.split('-').map(str::len).collect::<Vec<_>>(), [5; 5]);
        let plain = parse_code(&shown).unwrap();
        assert_eq!(display(&plain), shown);
        assert_ne!(new_code().unwrap(), shown);
    }

    #[test]
    fn codes_are_read_leniently_and_typos_are_caught() {
        let plain = parse_code(&new_code().unwrap()).unwrap();
        let loose = format!(" {} ", display(&plain).to_lowercase().replace('-', " - "));
        assert_eq!(parse_code(&loose), Ok(plain.clone()));
        // O for 0, I and L for 1.
        let zero_one: String = plain.chars().map(|c| if c == '0' { 'O' } else { c }).collect();
        assert_eq!(parse_code(&zero_one), Ok(plain.clone()));
        let ones: String =
            plain.chars().enumerate().map(|(i, c)| if c == '1' { ['I', 'l'][i % 2] } else { c }).collect();
        assert_eq!(parse_code(&ones), Ok(plain.clone()));
        // Every single wrong character, at every place.
        for at in 0..LEN {
            for &other in ALPHABET {
                let mut typo = plain.clone().into_bytes();
                if typo[at] == other {
                    continue;
                }
                typo[at] = other;
                assert_eq!(parse_code(&String::from_utf8(typo).unwrap()), Err(TYPO), "{at} {}", other as char);
            }
        }
        for bad in ["", &plain[..24], &format!("{plain}0"), &plain.replacen(&plain[..1], "U", 1), "ÇÇÇÇÇ"] {
            assert_eq!(parse_code(bad), Err(TYPO), "{bad}");
        }
    }

    /// Vectors worked out with Python's hmac (RFC 5869 by hand), not with this code.
    #[test]
    fn the_keys_are_fixed_for_a_code() {
        let keys = Keys::derive("0123456789ABCDEFGHJKMNPQX");
        let hex = |bytes: &[u8]| bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
        assert_eq!(keys.id_hex(), "04110f0258f5b6db9526c01091b62eab");
        assert_eq!(hex(&keys.auth), "653e78092adfc937650a948efafe54d7298edb681b701bf17283ec4ffb138ac9");
        assert_eq!(hex(&keys.key), "ab64a758fcc7549c8fd1ed5626868f68f10043675b27510d3129b5494f2d78c0");
    }
}
