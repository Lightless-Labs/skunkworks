#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    InvalidByte,
    InvalidPadding,
    InvalidLength,
    Finished,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamError { Finished }
fn alphabet(url: bool) -> &'static [u8; 64] {
    if url { b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_" }
    else { b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/" }
}
fn encode_block(bytes: &[u8], url: bool, padded: bool) -> String {
    let table = alphabet(url);
    let mut out = String::new();
    for b in bytes.chunks(3) {
        out.push(table[(b[0] >> 2) as usize] as char);
        out.push(table[(((b[0] & 3) << 4) | (if b.len() > 1 { b[1] >> 4 } else { 0 })) as usize] as char);
        if b.len() > 1 {
            out.push(table[(((b[1] & 15) << 2) | (if b.len() > 2 { b[2] >> 6 } else { 0 })) as usize] as char);
        } else if padded { out.push('='); }
        if b.len() > 2 { out.push(table[(b[2] & 63) as usize] as char); }
        else if padded { out.push('='); }
    }
    out
}
pub fn encode(input: &[u8]) -> String {
    encode_block(input, false, true)
}
pub struct Encoder {
    pending: Vec<u8>,
    done: bool,
    url: bool,
    padded: bool,
}
impl Encoder {
    pub fn new() -> Self {
        Self { pending: Vec::new(), done: false, url: false, padded: true }
    }
    pub fn push(&mut self, input: &[u8]) -> Result<String, StreamError> {
        if self.done { return Err(StreamError::Finished); }
        self.pending.extend_from_slice(input);
        let count = self.pending.len() / 3 * 3;
        let out = encode_block(&self.pending[..count], self.url, self.padded);
        self.pending.drain(..count);
        Ok(out)
    }
    pub fn finish(&mut self) -> Result<String, StreamError> {
        if self.done { return Err(StreamError::Finished); }
        self.done = true;
        Ok(encode_block(&self.pending, self.url, self.padded))
    }
}
pub struct Decoder {
    values: [u8; 4],
    positions: [usize; 4],
    len: usize,
    offset: usize,
    ended: bool,
    done: bool,
    failed: Option<DecodeError>,
    ignore: bool,
    url: bool,
    padded: bool,
}
impl Decoder {
    pub fn new() -> Self {
        Self { values: [0; 4], positions: [0; 4], len: 0, offset: 0,
            ended: false, done: false, failed: None, ignore: false, url: false, padded: true }
    }
    fn fail<T>(&mut self, error: DecodeError) -> Result<T, DecodeError> {
        self.failed = Some(error);
        Err(error)
    }
    pub fn push(&mut self, input: &str) -> Result<Vec<u8>, DecodeError> {
        if let Some(error) = self.failed { return Err(error); }
        if self.done { return Err(DecodeError::Finished); }
        let base = self.offset;
        self.offset += input.len();
        let mut out = Vec::new();
        for (i, byte) in input.bytes().enumerate() {
            let at = base + i;
            if self.ignore && matches!(byte, b' ' | b'\t' | b'\r' | b'\n' | 0x0b | 0x0c) { continue; }
            if self.ended { return self.fail(DecodeError::InvalidPadding); }
            let value;
            if byte == b'=' {
                if !self.padded || self.len < 2 { return self.fail(DecodeError::InvalidPadding); }
                value = 64;
            } else {
                if self.len == 3 && self.values[2] == 64 { return self.fail(DecodeError::InvalidPadding); }
                value = match alphabet(self.url).iter().position(|&v| v == byte) {
                    Some(v) => v as u8,
                    None => return self.fail(DecodeError::InvalidByte),
                };
            }
            self.values[self.len] = value;
            self.positions[self.len] = at;
            self.len += 1;
            if self.len == 4 {
                out.push((self.values[0] << 2) | (self.values[1] >> 4));
                if self.values[2] != 64 {
                    out.push((self.values[1] << 4) | (self.values[2] >> 2));
                    if self.values[3] != 64 { out.push((self.values[2] << 6) | self.values[3]); }
                }
                self.ended = self.values[3] == 64;
                self.len = 0;
            }
        }
        Ok(out)
    }
    pub fn finish(&mut self) -> Result<Vec<u8>, DecodeError> {
        if let Some(error) = self.failed { return Err(error); }
        if self.done { return Err(DecodeError::Finished); }
        if self.len != 0 && (self.padded || self.len == 1) {
            return self.fail(DecodeError::InvalidLength);
        }
        let mut out = Vec::new();
        self.done = true;
        Ok(out)
    }
}
pub fn decode(input: &str) -> Result<Vec<u8>, DecodeError> {
    let mut decoder = Decoder::new();
    let mut out = decoder.push(input)?;
    out.extend(decoder.finish()?);
    Ok(out)
}
