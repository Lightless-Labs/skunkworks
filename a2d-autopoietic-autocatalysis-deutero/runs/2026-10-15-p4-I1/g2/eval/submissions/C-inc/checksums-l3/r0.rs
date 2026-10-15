#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hashes {
    pub crc32: u32,
    pub adler32: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpanError {
    Reversed { span: usize },
    OutOfBounds { span: usize, offset: usize },
    Overlap { span: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotError {
    Shape,
    Hex { offset: usize },
    Adler { component: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Crc16 {
    Xmodem,
    False,
    X25,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Multi {
    pub base: Hashes,
    pub fletcher16: u16,
    pub fletcher32: u32,
    pub crc16: u16,
}

const ADLER_MOD: u32 = 65_521;

#[derive(Debug, Clone)]
struct ChecksumState {
    crc: u32,
    a: u32,
    b: u32,
}

impl ChecksumState {
    fn new() -> Self {
        Self {
            crc: 0xffff_ffff,
            a: 1,
            b: 0,
        }
    }

    fn from_seed(crc32: u32, adler32: u32) -> Result<Self, SnapshotError> {
        let a = adler32 & 0xffff;
        let b = adler32 >> 16;
        if a >= ADLER_MOD {
            return Err(SnapshotError::Adler { component: 0 });
        }
        if b >= ADLER_MOD {
            return Err(SnapshotError::Adler { component: 1 });
        }
        Ok(Self {
            crc: crc32 ^ 0xffff_ffff,
            a,
            b,
        })
    }

    fn update(&mut self, data: &[u8]) {
        for &byte in data {
            self.crc ^= byte as u32;
            for _ in 0..8 {
                let low = self.crc & 1;
                self.crc >>= 1;
                if low != 0 {
                    self.crc ^= 0xedb8_8320;
                }
            }

            self.a = (self.a + byte as u32) % ADLER_MOD;
            self.b = (self.b + self.a) % ADLER_MOD;
        }
    }

    fn checksum(&self) -> Hashes {
        Hashes {
            crc32: self.crc ^ 0xffff_ffff,
            adler32: (self.b << 16) | self.a,
        }
    }
}

fn validate_spans(data_len: usize, spans: &[Span]) -> Result<(), SpanError> {
    let mut previous_end = 0usize;

    for (index, span) in spans.iter().enumerate() {
        if span.start > span.end {
            return Err(SpanError::Reversed { span: index });
        }
        if span.start > data_len {
            return Err(SpanError::OutOfBounds {
                span: index,
                offset: span.start,
            });
        }
        if span.end > data_len {
            return Err(SpanError::OutOfBounds {
                span: index,
                offset: span.end,
            });
        }
        if index > 0 && span.start < previous_end {
            return Err(SpanError::Overlap { span: index });
        }
        previous_end = span.end;
    }

    Ok(())
}

pub fn crc32(data: &[u8]) -> u32 {
    let mut state = ChecksumState::new();
    state.update(data);
    state.checksum().crc32
}

pub fn adler32(data: &[u8]) -> u32 {
    let mut state = ChecksumState::new();
    state.update(data);
    state.checksum().adler32
}

pub fn checksums(data: &[u8]) -> Hashes {
    let mut state = ChecksumState::new();
    state.update(data);
    state.checksum()
}

pub fn checksums_spans(data: &[u8], spans: &[Span]) -> Result<Hashes, SpanError> {
    validate_spans(data.len(), spans)?;
    let mut state = ChecksumState::new();
    for span in spans {
        state.update(&data[span.start..span.end]);
    }
    Ok(state.checksum())
}

pub fn fletcher16(data: &[u8]) -> u16 {
    let mut a = 0u32;
    let mut b = 0u32;
    for &byte in data {
        a = (a + byte as u32) % 255;
        b = (b + a) % 255;
    }
    ((b << 8) | a) as u16
}

#[derive(Debug, Clone)]
struct Fletcher32State {
    a: u32,
    b: u32,
    pending_high: Option<u8>,
}

impl Fletcher32State {
    fn new() -> Self {
        Self {
            a: 0,
            b: 0,
            pending_high: None,
        }
    }

    fn update(&mut self, data: &[u8]) {
        for &byte in data {
            if let Some(high) = self.pending_high.take() {
                self.process_word(((high as u16) << 8) | byte as u16);
            } else {
                self.pending_high = Some(byte);
            }
        }
    }

    fn process_word(&mut self, word: u16) {
        self.a = (self.a + word as u32) % 65_535;
        self.b = (self.b + self.a) % 65_535;
    }

    fn finish(&mut self) -> u32 {
        if let Some(high) = self.pending_high.take() {
            self.process_word((high as u16) << 8);
        }
        (self.b << 16) | self.a
    }
}

pub fn fletcher32(data: &[u8]) -> u32 {
    let mut state = Fletcher32State::new();
    state.update(data);
    state.finish()
}

pub fn fletcher32_spans(data: &[u8], spans: &[Span]) -> Result<u32, SpanError> {
    validate_spans(data.len(), spans)?;
    let mut state = Fletcher32State::new();
    for span in spans {
        state.update(&data[span.start..span.end]);
    }
    Ok(state.finish())
}

fn crc16_update(mut crc: u16, data: &[u8], variant: Crc16) -> u16 {
    match variant {
        Crc16::Xmodem | Crc16::False => {
            for &byte in data {
                crc ^= (byte as u16) << 8;
                for _ in 0..8 {
                    if crc & 0x8000 != 0 {
                        crc = (crc << 1) ^ 0x1021;
                    } else {
                        crc <<= 1;
                    }
                }
            }
        }
        Crc16::X25 => {
            for &byte in data {
                crc ^= byte as u16;
                for _ in 0..8 {
                    if crc & 1 != 0 {
                        crc = (crc >> 1) ^ 0x8408;
                    } else {
                        crc >>= 1;
                    }
                }
            }
        }
    }
    crc
}

pub fn crc16(data: &[u8], variant: Crc16) -> u16 {
    let initial = match variant {
        Crc16::Xmodem => 0,
        Crc16::False | Crc16::X25 => 0xffff,
    };
    let crc = crc16_update(initial, data, variant);
    match variant {
        Crc16::X25 => crc ^ 0xffff,
        Crc16::Xmodem | Crc16::False => crc,
    }
}

pub fn multi_checksums(
    data: &[u8],
    spans: &[Span],
    variant: Crc16,
) -> Result<Multi, SpanError> {
    validate_spans(data.len(), spans)?;

    let mut base = ChecksumState::new();
    let mut f16_a = 0u32;
    let mut f16_b = 0u32;
    let mut f32 = Fletcher32State::new();
    let mut c16 = match variant {
        Crc16::Xmodem => 0,
        Crc16::False | Crc16::X25 => 0xffff,
    };

    for span in spans {
        let selected = &data[span.start..span.end];
        base.update(selected);

        for &byte in selected {
            f16_a = (f16_a + byte as u32) % 255;
            f16_b = (f16_b + f16_a) % 255;
        }
        f32.update(selected);
        c16 = crc16_update(c16, selected, variant);
    }

    if variant == Crc16::X25 {
        c16 ^= 0xffff;
    }

    Ok(Multi {
        base: base.checksum(),
        fletcher16: ((f16_b << 8) | f16_a) as u16,
        fletcher32: f32.finish(),
        crc16: c16,
    })
}

#[derive(Debug, Clone)]
pub struct Stream {
    state: ChecksumState,
}

impl Stream {
    pub fn new() -> Self {
        Self {
            state: ChecksumState::new(),
        }
    }

    pub fn from_seed(crc32: u32, adler32: u32) -> Result<Self, SnapshotError> {
        Ok(Self {
            state: ChecksumState::from_seed(crc32, adler32)?,
        })
    }

    pub fn checksum(&self) -> Hashes {
        self.state.checksum()
    }

    pub fn update(&mut self, data: &[u8]) {
        self.state.update(data);
    }

    pub fn update_spans(
        &mut self,
        data: &[u8],
        spans: &[Span],
    ) -> Result<(), SpanError> {
        validate_spans(data.len(), spans)?;
        for span in spans {
            self.state.update(&data[span.start..span.end]);
        }
        Ok(())
    }

    pub fn reset(&mut self) {
        self.state = ChecksumState::new();
    }

    pub fn snapshot(&self) -> String {
        let hashes = self.checksum();
        format!("c{:08x}:a{:08x}", hashes.crc32, hashes.adler32)
    }

    pub fn restore(token: &str) -> Result<Self, SnapshotError> {
        let bytes = token.as_bytes();
        if bytes.len() != 19 || bytes[0] != b'c' || bytes[9] != b':' || bytes[10] != b'a' {
            return Err(SnapshotError::Shape);
        }

        let mut crc = 0u32;
        let mut adler = 0u32;
        for offset in 1..9 {
            let digit = hex_value(bytes[offset]).ok_or(SnapshotError::Hex { offset })?;
            crc = (crc << 4) | digit as u32;
        }
        for offset in 11..19 {
            let digit = hex_value(bytes[offset]).ok_or(SnapshotError::Hex { offset })?;
            adler = (adler << 4) | digit as u32;
        }

        Self::from_seed(crc, adler)
    }
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

fn main() {}