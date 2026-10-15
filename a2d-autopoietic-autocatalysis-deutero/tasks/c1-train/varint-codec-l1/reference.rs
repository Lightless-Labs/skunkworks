#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    StartOutOfBounds { offset: usize },
    Truncated { offset: usize, start: usize },
    Overflow { offset: usize, start: usize },
}
pub fn encode_u64(mut value: u64) -> Vec<u8> {
    let mut bytes = Vec::new();
    loop {
        let mut byte = (value & 127) as u8;
        value >>= 7;
        if value != 0 { byte |= 128; }
        bytes.push(byte);
        if value == 0 { return bytes; }
    }
}
pub fn decode_u64(input: &[u8], start: usize) -> Result<(u64, usize), Error> {
    if start > input.len() { return Err(Error::StartOutOfBounds { offset: start }); }
    let mut value = 0u64;
    let mut cursor = start;
    for group in 0..10 {
        let byte = *input.get(cursor).ok_or(Error::Truncated { offset: input.len(), start })?;
        let payload = byte & 127;
        if group == 9 && (payload > 1 || byte & 128 != 0) {
            return Err(Error::Overflow { offset: cursor, start });
        }
        value |= (payload as u64) << (7 * group);
        cursor += 1;
        if byte & 128 == 0 { return Ok((value, cursor)); }
    }
    unreachable!()
}
pub fn decode_many(input: &[u8], start: usize, count: usize) -> Result<(Vec<u64>, usize), Error> {
    if start > input.len() { return Err(Error::StartOutOfBounds { offset: start }); }
    let mut values = Vec::new();
    let mut cursor = start;
    for _ in 0..count {
        let (value, next) = decode_u64(input, cursor)?;
        values.push(value);
        cursor = next;
    }
    Ok((values, cursor))
}
