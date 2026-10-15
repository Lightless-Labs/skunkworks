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

pub fn encode_i64(value: i64) -> Vec<u8> {
    let zigzag = ((value as u64) << 1) ^ ((value >> 63) as u64);
    encode_u64(zigzag)
}
pub fn decode_i64(input: &[u8], start: usize) -> Result<(i64, usize), Error> {
    let (value, next) = decode_u64(input, start)?;
    Ok((((value >> 1) as i64) ^ -((value & 1) as i64), next))
}
#[derive(Clone)]
pub struct StreamDecoder {
    position: usize,
    pending: usize,
    value: u64,
    start: usize,
}
impl StreamDecoder {
    pub fn new() -> Self { Self { position: 0, pending: 0, value: 0, start: 0 } }
    pub fn position(&self) -> usize { self.position }
    pub fn pending_len(&self) -> usize { self.pending }
    pub fn reset(&mut self) { *self = Self::new(); }
    pub fn finish(&self) -> Result<(), Error> {
        if self.pending == 0 { Ok(()) }
        else { Err(Error::Truncated { offset: self.position, start: self.position - self.pending }) }
    }
    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<u64>, Error> {
        let mut next = self.clone();
        let mut values = Vec::new();
        for &byte in chunk {
            if next.pending == 0 { next.start = next.position; }
            let payload = byte & 127;
            if next.pending == 9 && payload > 1 {
                return Err(Error::Overflow { offset: next.position, start: next.start });
            }
            next.value |= (payload as u64) << (7 * next.pending);
            next.pending += 1;
            next.position += 1;
            if byte & 128 == 0 {
                values.push(next.value);
                next.pending = 0;
                next.value = 0;
            }
        }
        *self = next;
        Ok(values)
    }
}
