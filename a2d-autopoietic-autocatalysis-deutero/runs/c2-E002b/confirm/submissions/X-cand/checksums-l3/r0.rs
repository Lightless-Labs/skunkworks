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

#[derive(Debug, Clone)]
pub struct Stream {
    crc: u32,
    a: u32,
    b: u32,
}

impl Stream {
    pub fn new() -> Self {
        Self {
            crc: 0xffff_ffff,
            a: 1,
            b: 0,
        }
    }

    pub fn from_seed(crc32: u32, adler32: u32) -> Result<Self, SnapshotError> {
        let a = adler32 & 0xffff;
        let b = adler32 >> 16;
        if a >= 65_521 {
            return Err(SnapshotError::Adler { component: 0 });
        }
        if b >= 65_521 {
            return Err(SnapshotError::Adler { component: 1 });
        }
        Ok(Self {
            crc: crc32 ^ 0xffff_ffff,
            a,
            b,
        })
    }

    pub fn checksum(&self) -> Hashes {
        Hashes {
            crc32: self.crc ^ 0xffff_ffff,
            adler32: (self.b << 16) | self.a,
        }
    }

    pub fn update(&mut self, data: &[u8]) {
        for &byte in data {
            self.crc = crc32_byte(self.crc, byte);
            self.adler_byte(byte);
        }
    }

    pub fn update_spans(
        &mut self,
        data: &[u8],
        spans: &[Span],
    ) -> Result<(), SpanError> {
        validate_spans(data.len(), spans)?;

        for span in spans {
            for &byte in &data[span.start..span.end] {
                self.crc = crc32_byte(self.crc, byte);
                self.adler_byte(byte);
            }
        }
        Ok(())
    }

    pub fn reset(&mut self) {
        *self = Self::new();
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
        for (i, &byte) in bytes.iter().enumerate() {
            if (1..=8).contains(&i) {
                crc = (crc << 4) | u32::from(hex_nibble(byte).ok_or(
                    SnapshotError::Hex { offset: i },
                )?);
            } else if (11..=18).contains(&i) {
                adler = (adler << 4) | u32::from(hex_nibble(byte).ok_or(
                    SnapshotError::Hex { offset: i },
                )?);
            }
        }

        Self::from_seed(crc, adler)
    }

    fn adler_byte(&mut self, byte: u8) {
        self.a = (self.a + u32::from(byte)) % 65_521;
        self.b = (self.b + self.a) % 65_521;
    }
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

fn crc32_byte(mut crc: u32, byte: u8) -> u32 {
    crc ^= u32::from(byte);
    for _ in 0..8 {
        let low = crc & 1;
        crc >>= 1;
        if low != 0 {
            crc ^= 0xedb8_8320;
        }
    }
    crc
}

pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffff;
    for &byte in data {
        crc = crc32_byte(crc, byte);
    }
    crc ^ 0xffff_ffff
}

pub fn adler32(data: &[u8]) -> u32 {
    let mut a = 1u32;
    let mut b = 0u32;
    for &byte in data {
        a = (a + u32::from(byte)) % 65_521;
        b = (b + a) % 65_521;
    }
    (b << 16) | a
}

pub fn checksums(data: &[u8]) -> Hashes {
    Hashes {
        crc32: crc32(data),
        adler32: adler32(data),
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

pub fn checksums_spans(data: &[u8], spans: &[Span]) -> Result<Hashes, SpanError> {
    validate_spans(data.len(), spans)?;
    let mut stream = Stream::new();
    stream.update_spans(data, spans)?;
    Ok(stream.checksum())
}

pub fn fletcher16(data: &[u8]) -> u16 {
    let mut a = 0u32;
    let mut b = 0u32;
    for &byte in data {
        a = (a + u32::from(byte)) % 255;
        b = (b + a) % 255;
    }
    ((b as u16) << 8) | a as u16
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Fletcher32State {
    a: u64,
    b: u64,
    pending: Option<u8>,
}

impl Fletcher32State {
    fn new() -> Self {
        Self {
            a: 0,
            b: 0,
            pending: None,
        }
    }

    fn byte(&mut self, byte: u8) {
        match self.pending.take() {
            None => self.pending = Some(byte),
            Some(high) => self.word((u16::from(high) << 8) | u16::from(byte)),
        }
    }

    fn word(&mut self, word: u16) {
        self.a = (self.a + u64::from(word)) % 65_535;
        self.b = (self.b + self.a) % 65_535;
    }

    fn finish(mut self) -> u32 {
        if let Some(high) = self.pending {
            self.word(u16::from(high) << 8);
        }
        ((self.b as u32) << 16) | self.a as u32
    }
}

pub fn fletcher32(data: &[u8]) -> u32 {
    let mut state = Fletcher32State::new();
    for &byte in data {
        state.byte(byte);
    }
    state.finish()
}

pub fn fletcher32_spans(data: &[u8], spans: &[Span]) -> Result<u32, SpanError> {
    validate_spans(data.len(), spans)?;
    let mut state = Fletcher32State::new();
    for span in spans {
        for &byte in &data[span.start..span.end] {
            state.byte(byte);
        }
    }
    Ok(state.finish())
}

pub fn crc16(data: &[u8], variant: Crc16) -> u16 {
    let mut crc = match variant {
        Crc16::Xmodem => 0u16,
        Crc16::False | Crc16::X25 => 0xffff,
    };

    match variant {
        Crc16::Xmodem | Crc16::False => {
            for &byte in data {
                crc ^= u16::from(byte) << 8;
                for _ in 0..8 {
                    let top = crc & 0x8000;
                    crc = crc.wrapping_shl(1);
                    if top != 0 {
                        crc ^= 0x1021;
                    }
                }
            }
            crc
        }
        Crc16::X25 => {
            for &byte in data {
                crc ^= u16::from(byte);
                for _ in 0..8 {
                    let low = crc & 1;
                    crc >>= 1;
                    if low != 0 {
                        crc ^= 0x8408;
                    }
                }
            }
            crc ^ 0xffff
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Crc16State {
    register: u16,
    variant: Crc16,
}

impl Crc16State {
    fn new(variant: Crc16) -> Self {
        let register = match variant {
            Crc16::Xmodem => 0,
            Crc16::False | Crc16::X25 => 0xffff,
        };
        Self { register, variant }
    }

    fn byte(&mut self, byte: u8) {
        match self.variant {
            Crc16::Xmodem | Crc16::False => {
                self.register ^= u16::from(byte) << 8;
                for _ in 0..8 {
                    let top = self.register & 0x8000;
                    self.register = self.register.wrapping_shl(1);
                    if top != 0 {
                        self.register ^= 0x1021;
                    }
                }
            }
            Crc16::X25 => {
                self.register ^= u16::from(byte);
                for _ in 0..8 {
                    let low = self.register & 1;
                    self.register >>= 1;
                    if low != 0 {
                        self.register ^= 0x8408;
                    }
                }
            }
        }
    }

    fn finish(self) -> u16 {
        match self.variant {
            Crc16::X25 => self.register ^ 0xffff,
            Crc16::Xmodem | Crc16::False => self.register,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MultiState {
    crc: u32,
    adler_a: u32,
    adler_b: u32,
    fletcher16_a: u32,
    fletcher16_b: u32,
    fletcher32: Fletcher32State,
    crc16: Crc16State,
}

impl MultiState {
    fn new(variant: Crc16) -> Self {
        Self {
            crc: 0xffff_ffff,
            adler_a: 1,
            adler_b: 0,
            fletcher16_a: 0,
            fletcher16_b: 0,
            fletcher32: Fletcher32State::new(),
            crc16: Crc16State::new(variant),
        }
    }

    fn byte(&mut self, byte: u8) {
        self.crc = crc32_byte(self.crc, byte);
        self.adler_a = (self.adler_a + u32::from(byte)) % 65_521;
        self.adler_b = (self.adler_b + self.adler_a) % 65_521;

        self.fletcher16_a = (self.fletcher16_a + u32::from(byte)) % 255;
        self.fletcher16_b = (self.fletcher16_b + self.fletcher16_a) % 255;

        self.fletcher32.byte(byte);
        self.crc16.byte(byte);
    }

    fn finish(self) -> Multi {
        Multi {
            base: Hashes {
                crc32: self.crc ^ 0xffff_ffff,
                adler32: (self.adler_b << 16) | self.adler_a,
            },
            fletcher16: ((self.fletcher16_b as u16) << 8) | self.fletcher16_a as u16,
            fletcher32: self.fletcher32.finish(),
            crc16: self.crc16.finish(),
        }
    }
}

pub fn multi_checksums(
    data: &[u8],
    spans: &[Span],
    variant: Crc16,
) -> Result<Multi, SpanError> {
    validate_spans(data.len(), spans)?;
    let mut state = MultiState::new(variant);
    for span in spans {
        for &byte in &data[span.start..span.end] {
            state.byte(byte);
        }
    }
    Ok(state.finish())
}