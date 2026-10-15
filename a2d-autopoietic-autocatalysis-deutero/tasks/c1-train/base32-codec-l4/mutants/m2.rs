#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum Alphabet { Standard, Hex }
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum Padding { Required, Omitted }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Config { pub alphabet: Alphabet, pub padding: Padding, pub canonical: bool }
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamError {
    InvalidByte { offset: usize, byte: u8 },
    NonCanonical { offset: usize, expected: u8 },
    Padding { offset: usize },
    InvalidLength { offset: usize },
    TrailingBits { offset: usize },
    Closed,
}
const STD: &[u8;32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
const HEX: &[u8;32] = b"0123456789ABCDEFGHIJKLMNOPQRSTUV";
fn table(a: Alphabet) -> &'static [u8;32] { match a { Alphabet::Standard => STD, Alphabet::Hex => HEX } }
fn pad_count(d: usize) -> Option<usize> {
    match d % 8 { 0 => Some(0), 2 => Some(6), 4 => Some(4), 5 => Some(3), 7 => Some(1), _ => None }
}
fn fragment(input: &[u8], alphabet: &[u8;32], padded: bool) -> String {
    let mut out = String::new();
    let mut bits = 0u32;
    let mut count = 0u32;
    for &byte in input {
        bits = (bits << 8) | byte as u32;
        count += 8;
        while count >= 5 { count -= 5; out.push(alphabet[((bits >> count) & 31) as usize] as char); }
        bits &= (1 << count) - 1;
    }
    if count != 0 { out.push(alphabet[((bits << (5-count)) & 31) as usize] as char); }
    if padded { while out.len() % 8 != 0 { out.push('='); } }
    out
}
pub struct Encoder { config: Config, pending: Vec<u8>, closed: bool }
impl Encoder {
    pub fn new(config: Config) -> Self { Self { config, pending: Vec::new(), closed: false } }
    pub fn push(&mut self, input: &[u8]) -> Result<String, StreamError> {
        if self.closed { return Err(StreamError::Closed); }
        self.pending.extend_from_slice(input);
        let end = self.pending.len() / 5 * 5;
        let output = fragment(&self.pending[..end], table(self.config.alphabet), false);
        self.pending.drain(..end);
        Ok(output)
    }
    pub fn finish(&mut self) -> Result<String, StreamError> {
        if self.closed { return Err(StreamError::Closed); }
        let output = fragment(&self.pending, table(self.config.alphabet), self.config.padding == Padding::Required);
        self.pending.clear();
        self.closed = true;
        Ok(output)
    }
}
#[derive(Clone)]
pub struct Decoder {
    config: Config,
    closed: bool,
    total: usize,
    data_count: usize,
    pads: usize,
    first_pad: Option<usize>,
    last_data: Option<usize>,
    bits: u32,
    count: u32,
}
impl Decoder {
    pub fn new(config: Config) -> Self {
        Self { config, closed: false, total: 0, data_count: 0, pads: 0, first_pad: None, last_data: None, bits: 0, count: 0 }
    }
    pub fn push(&mut self, input: &str) -> Result<Vec<u8>, StreamError> {
        if self.closed { return Err(StreamError::Closed); }
        let mut trial = self.clone();
        let output = trial.accept(input)?;
        *self = trial;
        Ok(output)
    }
    fn accept(&mut self, input: &str) -> Result<Vec<u8>, StreamError> {
        let mut out = Vec::new();
        for (local, byte) in input.bytes().enumerate() {
            let offset = local;
            if byte == b'=' {
                if self.config.padding == Padding::Omitted { return Err(StreamError::Padding { offset }); }
                let expected = pad_count(self.data_count).unwrap_or(0);
                if expected == 0 || self.pads >= expected { return Err(StreamError::Padding { offset }); }
                if self.first_pad.is_none() { self.first_pad = Some(offset); }
                self.pads += 1;
                continue;
            }
            let upper = byte.to_ascii_uppercase();
            let value = table(self.config.alphabet).iter().position(|&b| b == upper)
                .ok_or(StreamError::InvalidByte { offset, byte })? as u32;
            if self.config.canonical && byte != upper { return Err(StreamError::NonCanonical { offset, expected: upper }); }
            if self.first_pad.is_some() { return Err(StreamError::Padding { offset }); }
            self.data_count += 1;
            self.last_data = Some(offset);
            self.bits = (self.bits << 5) | value;
            self.count += 5;
            if self.count >= 8 {
                self.count -= 8;
                out.push((self.bits >> self.count) as u8);
            }
            self.bits &= (1 << self.count) - 1;
        }
        self.total += input.len();
        Ok(out)
    }
    pub fn finish(&mut self) -> Result<Vec<u8>, StreamError> {
        if self.closed { return Err(StreamError::Closed); }
        let expected = pad_count(self.data_count).ok_or(StreamError::InvalidLength { offset: self.total })?;
        if self.config.padding == Padding::Required && self.pads != expected {
            return Err(StreamError::Padding { offset: self.first_pad.unwrap_or(self.total) });
        }
        if self.bits != 0 { return Err(StreamError::TrailingBits { offset: self.last_data.unwrap() }); }
        self.closed = true;
        Ok(Vec::new())
    }
}
