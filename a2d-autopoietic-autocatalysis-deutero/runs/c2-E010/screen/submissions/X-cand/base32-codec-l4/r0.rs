#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alphabet { Standard, Hex }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Padding { Required, Omitted }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Config {
    pub alphabet: Alphabet,
    pub padding: Padding,
    pub canonical: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamError {
    InvalidByte { offset: usize, byte: u8 },
    NonCanonical { offset: usize, expected: u8 },
    Padding { offset: usize },
    InvalidLength { offset: usize },
    TrailingBits { offset: usize },
    Closed,
}

pub struct Encoder {
    config: Config,
    pending: Vec<u8>,
    closed: bool,
}

pub struct Decoder {
    config: Config,
    pos: usize,
    symbols: usize,
    accumulator: u64,
    bits: u8,
    last: u8,
    last_pos: usize,
    padding_started: bool,
    pad_start: Option<usize>,
    pads: usize,
    pad_target: usize,
    closed: bool,
}

fn encode_symbol(a: Alphabet, v: u8) -> char {
    match a {
        Alphabet::Standard => b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567"[v as usize] as char,
        Alphabet::Hex => b"0123456789ABCDEFGHIJKLMNOPQRSTUV"[v as usize] as char,
    }
}
fn decode_symbol(a: Alphabet, b: u8) -> Option<u8> {
    match a {
        Alphabet::Standard => match b {
            b'A'..=b'Z' => Some(b - b'A'),
            b'2'..=b'7' => Some(b - b'2' + 26),
            _ => None,
        },
        Alphabet::Hex => match b {
            b'0'..=b'9' => Some(b - b'0'),
            b'A'..=b'V' => Some(b - b'A' + 10),
            _ => None,
        },
    }
}
fn pad_for(n: usize) -> Option<usize> {
    match n % 8 { 0 => Some(0), 2 => Some(6), 4 => Some(4), 5 => Some(3), 7 => Some(1), _ => None }
}

impl Encoder {
    pub fn new(config: Config) -> Self { Self { config, pending: Vec::new(), closed: false } }
    pub fn push(&mut self, input: &[u8]) -> Result<String, StreamError> {
        if self.closed { return Err(StreamError::Closed); }
        let mut bytes = Vec::with_capacity(self.pending.len() + input.len());
        bytes.extend_from_slice(&self.pending);
        bytes.extend_from_slice(input);
        let full = bytes.len() / 5 * 5;
        let mut out = String::with_capacity(full / 5 * 8);
        let mut acc = 0u64;
        let mut bit_len = 0usize;
        for &b in &bytes[..full] {
            acc = (acc << 8) | b as u64;
            bit_len += 8;
            while bit_len >= 5 {
                bit_len -= 5;
                out.push(encode_symbol(self.config.alphabet, ((acc >> bit_len) & 31) as u8));
            }
            if bit_len == 0 { acc = 0; } else { acc &= (1u64 << bit_len) - 1; }
        }
        self.pending.clear();
        self.pending.extend_from_slice(&bytes[full..]);
        Ok(out)
    }
    pub fn finish(&mut self) -> Result<String, StreamError> {
        if self.closed { return Err(StreamError::Closed); }
        let mut out = String::new();
        if !self.pending.is_empty() {
            let mut acc = 0u64;
            for &b in &self.pending { acc = (acc << 8) | b as u64; }
            let bits = self.pending.len() * 8;
            let symbols = (bits + 4) / 5;
            for i in 0..symbols {
                let shift = bits.saturating_sub((i + 1) * 5);
                let value = if (i + 1) * 5 <= bits {
                    ((acc >> shift) & 31) as u8
                } else {
                    ((acc << ((i + 1) * 5 - bits)) & 31) as u8
                };
                out.push(encode_symbol(self.config.alphabet, value));
            }
            if self.config.padding == Padding::Required {
                while out.len() % 8 != 0 { out.push('='); }
            }
        }
        self.pending.clear();
        self.closed = true;
        Ok(out)
    }
}

impl Decoder {
    pub fn new(config: Config) -> Self {
        Self { config, pos: 0, symbols: 0, accumulator: 0, bits: 0, last: 0, last_pos: 0, padding_started: false, pad_start: None, pads: 0, pad_target: 0, closed: false }
    }
    pub fn push(&mut self, input: &str) -> Result<Vec<u8>, StreamError> {
        if self.closed { return Err(StreamError::Closed); }
        let mut t = Self {
            config: self.config, pos: self.pos, symbols: self.symbols,
            accumulator: self.accumulator, bits: self.bits, last: self.last,
            last_pos: self.last_pos, padding_started: self.padding_started,
            pad_start: self.pad_start, pads: self.pads, pad_target: self.pad_target,
            closed: self.closed,
        };
        let mut out = Vec::new();
        for &b in input.as_bytes() {
            let at = t.pos;
            let value = if b == b'=' { None } else {
                let upper = b.to_ascii_uppercase();
                let Some(v) = decode_symbol(t.config.alphabet, upper) else {
                    return Err(StreamError::InvalidByte { offset: at, byte: b });
                };
                if t.config.canonical && b != upper {
                    return Err(StreamError::NonCanonical { offset: at, expected: upper });
                }
                Some(v)
            };
            if b == b'=' {
                if t.config.padding == Padding::Omitted { return Err(StreamError::Padding { offset: at }); }
                if !t.padding_started {
                    let Some(count) = pad_for(t.symbols) else { return Err(StreamError::Padding { offset: at }); };
                    if count == 0 { return Err(StreamError::Padding { offset: at }); }
                    t.padding_started = true;
                    t.pad_start = Some(at);
                    t.pad_target = count;
                }
                if t.pads >= t.pad_target { return Err(StreamError::Padding { offset: at }); }
                t.pads += 1;
            } else {
                let v = value.unwrap();
                if t.padding_started { return Err(StreamError::Padding { offset: at }); }
                t.symbols = t.symbols.checked_add(1).ok_or(StreamError::InvalidLength { offset: at })?;
                t.last = v;
                t.last_pos = at;
                t.accumulator = (t.accumulator << 5) | v as u64;
                t.bits += 5;
                while t.bits >= 8 {
                    t.bits -= 8;
                    out.push((t.accumulator >> t.bits) as u8);
                    if t.bits == 0 { t.accumulator = 0; }
                    else { t.accumulator &= (1u64 << t.bits) - 1; }
                }
            }
            t.pos = t.pos.checked_add(1).ok_or(StreamError::InvalidLength { offset: at })?;
        }
        self.pos = t.pos;
        self.symbols = t.symbols;
        self.accumulator = t.accumulator;
        self.bits = t.bits;
        self.last = t.last;
        self.last_pos = t.last_pos;
        self.padding_started = t.padding_started;
        self.pad_start = t.pad_start;
        self.pads = t.pads;
        self.pad_target = t.pad_target;
        Ok(out)
    }
    pub fn finish(&mut self) -> Result<Vec<u8>, StreamError> {
        if self.closed { return Err(StreamError::Closed); }
        let Some(expected) = pad_for(self.symbols) else {
            return Err(StreamError::InvalidLength { offset: self.pos });
        };
        if self.config.padding == Padding::Required && self.pads != expected {
            return Err(StreamError::Padding { offset: self.pad_start.unwrap_or(self.pos) });
        }
        let unused = match self.symbols % 8 { 0 => 0, 2 => 2, 4 => 4, 5 => 1, 7 => 3, _ => 0 };
        if unused != 0 && (self.last & ((1u8 << unused) - 1)) != 0 {
            return Err(StreamError::TrailingBits { offset: self.last_pos });
        }
        self.closed = true;
        Ok(Vec::new())
    }
}
