#[derive(Debug, Clone, PartialEq, Eq)] pub enum DecodeError { InvalidByte { offset: usize, byte: u8 }, Padding { offset: usize }, InvalidLength { offset: usize }, TrailingBits { offset: usize } }

const STD: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
const HEX: &[u8; 32] = b"0123456789ABCDEFGHIJKLMNOPQRSTUV";
fn encode_core(input: &[u8], alphabet: &[u8; 32], padded: bool) -> String {
    let mut out = String::new();
    let mut bits = 0u32;
    let mut count = 0u32;
    for &byte in input {
        bits = (bits << 8) | byte as u32;
        count += 8;
        while count >= 5 {
            count -= 5;
            out.push(alphabet[((bits >> count) & 31) as usize] as char);
        }
        bits &= (1 << count) - 1;
    }
    if count != 0 { out.push(alphabet[((bits << (5-count)) & 31) as usize] as char); }
    if padded { while out.len() % 8 != 0 { out.push('='); } }
    out
}
fn expected_padding(n: usize) -> Option<usize> {
    match n % 8 { 0 => Some(0), 2 => Some(6), 4 => Some(4), 5 => Some(3), 7 => Some(1), _ => None }
}
fn decode_core(input: &str, alphabet: &[u8;32], padded: bool) -> Result<Vec<u8>, DecodeError> {
    let bytes = input.as_bytes();
    // Global lexical validation deliberately precedes structure checks.
    for (offset, &byte) in bytes.iter().enumerate() {
        if byte != b'=' && !alphabet.contains(&byte) {
            return Err(DecodeError::InvalidByte { offset, byte });
        }
    }
    let first = bytes.iter().position(|&b| b == b'=').unwrap_or(bytes.len());
    if !padded && first != bytes.len() { return Err(DecodeError::Padding { offset: first }); }
    if padded {
        for offset in first..bytes.len() {
            if bytes[offset] != b'=' { return Err(DecodeError::Padding { offset }); }
        }
    }
    let expected = expected_padding(first).ok_or(DecodeError::InvalidLength { offset: bytes.len() })?;
    if padded && (bytes.len()-first != expected || bytes.len()%8 != 0) {
        return Err(DecodeError::Padding { offset: first });
    }
    let mut output = Vec::new();
    let mut bits = 0u32;
    let mut count = 0u32;
    for &byte in &bytes[..first] {
        let v = alphabet.iter().position(|&x| x == byte).unwrap() as u32;
        bits = (bits << 5) | v;
        count += 5;
        if count >= 8 {
            count -= 8;
            output.push((bits >> count) as u8);
        }
        bits &= (1 << count) - 1;
    }
    if bits != 0 { return Err(DecodeError::TrailingBits { offset: first }); }
    Ok(output)
}

pub fn encode(input: &[u8]) -> String { encode_core(input, STD, true) }
pub fn decode(input: &str) -> Result<Vec<u8>, DecodeError> { decode_core(input, STD, true) }
