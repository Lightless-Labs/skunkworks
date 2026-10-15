#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    MissingCheck { offset: usize },
    InvalidByte { offset: usize, byte: u8 },
    InvalidLength { offset: usize },
    TrailingBits { offset: usize },
    CheckMismatch { offset: usize, expected: u8, found: u8 },
}
const DATA: &[u8;32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
const CHECK: &[u8;37] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ*~$=U";
fn value(byte: u8, check: bool) -> Option<u8> {
    let upper = byte.to_ascii_uppercase();
    let normalized = match upper { b'I' => b'1', b'O' => b'0', x => x };
    let alphabet: &[u8] = if check { CHECK } else { DATA };
    alphabet.iter().position(|&b| b == normalized).map(|v| v as u8)
}
pub fn encode(input: &[u8], checked: bool) -> String {
    let mut out = String::new();
    let mut bits = 0u32;
    let mut count = 0u32;
    let mut remainder = 0u32;
    for &byte in input {
        bits = (bits << 8) | byte as u32;
        count += 8;
        while count >= 5 {
            count -= 5;
            let v = (bits >> count) & 31;
            out.push(DATA[v as usize] as char);
            remainder = (remainder * 32 + v) % 37;
        }
        bits &= (1 << count) - 1;
    }
    if count != 0 {
        let v = (bits << (5-count)) & 31;
        out.push(DATA[v as usize] as char);
        remainder = (remainder * 32 + v) % 37;
    }
    if checked { out.push(CHECK[remainder as usize] as char); }
    out
}
pub fn decode(input: &str, checked: bool) -> Result<Vec<u8>, DecodeError> {
    let mut symbols: Vec<(usize,u8)> = input.bytes().enumerate().filter(|(_,b)| *b != b'-').collect();
    let check = if checked {
        Some(symbols.pop().ok_or(DecodeError::MissingCheck { offset: input.len() })?)
    } else { None };
    let mut data = Vec::new();
    for &(offset, byte) in &symbols {
        data.push(value(byte, false).ok_or(DecodeError::InvalidByte { offset, byte })?);
    }
    let check_value = match check {
        Some((offset,byte)) => Some(value(byte,true).ok_or(DecodeError::InvalidByte { offset, byte })?),
        None => None,
    };
    match data.len() % 8 { 0 | 2 | 4 | 5 | 7 => (), _ => return Err(DecodeError::InvalidLength { offset: input.len() }) }
    let mut output = Vec::new();
    let mut bits = 0u32;
    let mut count = 0u32;
    let mut remainder = 0u8;
    for &v in &data {
        remainder = ((remainder as u32 * 32 + v as u32) % 37) as u8;
        bits = (bits << 5) | v as u32;
        count += 5;
        if count >= 8 { count -= 8; output.push((bits >> count) as u8); }
        bits &= (1 << count) - 1;
    }
    if bits != 0 { return Err(DecodeError::TrailingBits { offset: symbols.last().unwrap().0 }); }
    if let (Some((offset,_)),Some(found)) = (check,check_value) {
        if found != remainder { return Err(DecodeError::CheckMismatch { offset, expected: remainder, found }); }
    }
    Ok(output)
}
