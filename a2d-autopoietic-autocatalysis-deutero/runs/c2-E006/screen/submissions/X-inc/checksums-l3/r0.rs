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

fn crc32_step(mut crc: u32, byte: u8) -> u32 {
    crc ^= byte as u32;
    for _ in 0..8 {
        crc = (crc >> 1) ^ if crc & 1 != 0 { 0xedb8_8320 } else { 0 };
    }
    crc
}

fn adler_step(a: &mut u32, b: &mut u32, byte: u8) {
    *a = (*a + byte as u32) % ADLER_MOD;
    *b = (*b + *a) % ADLER_MOD;
}

pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffff;
    for &byte in data {
        crc = crc32_step(crc, byte);
    }
    crc ^ 0xffff_ffff
}

pub fn adler32(data: &[u8]) -> u32 {
    let mut a = 1;
    let mut b = 0;
    for &byte in data {
        adler_step(&mut a, &mut b, byte);
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
    let mut previous_end = 0;
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
        if index != 0 && span.start < previous_end {
            return Err(SpanError::Overlap { span: index });
        }
        previous_end = span.end;
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct Accumulator {
    crc: u32,
    a: u32,
    b: u32,
    f16_a: u32,
    f16_b: u32,
    f32_a: u64,
    f32_b: u64,
    f32_pending: Option<u8>,
    c16: u16,
    c16_variant: Crc16,
}

impl Accumulator {
    fn new(c16_variant: Crc16) -> Self {
        Self {
            crc: 0xffff_ffff,
            a: 1,
            b: 0,
            f16_a: 0,
            f16_b: 0,
            f32_a: 0,
            f32_b: 0,
            f32_pending: None,
            c16: match c16_variant {
                Crc16::Xmodem => 0,
                Crc16::False | Crc16::X25 => 0xffff,
            },
            c16_variant,
        }
    }

    fn from_seed(crc32: u32, adler32: u32) -> Self {
        Self {
            crc: crc32 ^ 0xffff_ffff,
            a: adler32 & 0xffff,
            b: adler32 >> 16,
            f16_a: 0,
            f16_b: 0,
            f32_a: 0,
            f32_b: 0,
            f32_pending: None,
            c16: 0,
            c16_variant: Crc16::Xmodem,
        }
    }

    fn update_byte(&mut self, byte: u8) {
        self.crc = crc32_step(self.crc, byte);
        adler_step(&mut self.a, &mut self.b, byte);

        self.f16_a = (self.f16_a + byte as u32) % 255;
        self.f16_b = (self.f16_b + self.f16_a) % 255;

        if let Some(high) = self.f32_pending.take() {
            self.fletcher32_word(((high as u16) << 8) | byte as u16);
        } else {
            self.f32_pending = Some(byte);
        }

        match self.c16_variant {
            Crc16::Xmodem | Crc16::False => {
                self.c16 ^= (byte as u16) << 8;
                for _ in 0..8 {
                    let top = self.c16 & 0x8000 != 0;
                    self.c16 = self.c16.wrapping_shl(1);
                    if top {
                        self.c16 ^= 0x1021;
                    }
                }
            }
            Crc16::X25 => {
                self.c16 ^= byte as u16;
                for _ in 0..8 {
                    let low = self.c16 & 1 != 0;
                    self.c16 >>= 1;
                    if low {
                        self.c16 ^= 0x8408;
                    }
                }
            }
        }
    }

    fn fletcher32_word(&mut self, word: u16) {
        self.f32_a = (self.f32_a + word as u64) % 65_535;
        self.f32_b = (self.f32_b + self.f32_a) % 65_535;
    }

    fn finish_fletcher32(&mut self) {
        if let Some(high) = self.f32_pending.take() {
            self.fletcher32_word((high as u16) << 8);
        }
    }

    fn hashes(&self) -> Hashes {
        Hashes {
            crc32: self.crc ^ 0xffff_ffff,
            adler32: (self.b << 16) | self.a,
        }
    }

    fn fletcher16(&self) -> u16 {
        ((self.f16_b as u16) << 8) | self.f16_a as u16
    }

    fn fletcher32(&self) -> u32 {
        let mut copy = *self;
        copy.finish_fletcher32();
        ((copy.f32_b as u32) << 16) | copy.f32_a as u32
    }

    fn crc16(&self) -> u16 {
        match self.c16_variant {
            Crc16::X25 => self.c16 ^ 0xffff,
            Crc16::Xmodem | Crc16::False => self.c16,
        }
    }
}

pub fn checksums_spans(data: &[u8], spans: &[Span]) -> Result<Hashes, SpanError> {
    validate_spans(data.len(), spans)?;
    let mut acc = Accumulator::new(Crc16::Xmodem);
    for span in spans {
        for &byte in &data[span.start..span.end] {
            acc.update_byte(byte);
        }
    }
    Ok(acc.hashes())
}

pub fn fletcher16(data: &[u8]) -> u16 {
    let mut a = 0u32;
    let mut b = 0u32;
    for &byte in data {
        a = (a + byte as u32) % 255;
        b = (b + a) % 255;
    }
    ((b as u16) << 8) | a as u16
}

pub fn fletcher32(data: &[u8]) -> u32 {
    let mut a = 0u64;
    let mut b = 0u64;
    let mut chunks = data.chunks_exact(2);
    for pair in &mut chunks {
        let word = u16::from_be_bytes([pair[0], pair[1]]) as u64;
        a = (a + word) % 65_535;
        b = (b + a) % 65_535;
    }
    if let Some(&high) = chunks.remainder().first() {
        a = (a + ((high as u64) << 8)) % 65_535;
        b = (b + a) % 65_535;
    }
    ((b as u32) << 16) | a as u32
}

pub fn fletcher32_spans(data: &[u8], spans: &[Span]) -> Result<u32, SpanError> {
    validate_spans(data.len(), spans)?;
    let mut acc = Accumulator::new(Crc16::Xmodem);
    for span in spans {
        for &byte in &data[span.start..span.end] {
            acc.update_byte(byte);
        }
    }
    Ok(acc.fletcher32())
}

pub fn crc16(data: &[u8], variant: Crc16) -> u16 {
    let mut acc = Accumulator::new(variant);
    for &byte in data {
        acc.update_byte(byte);
    }
    acc.crc16()
}

pub fn multi_checksums(
    data: &[u8],
    spans: &[Span],
    variant: Crc16,
) -> Result<Multi, SpanError> {
    validate_spans(data.len(), spans)?;
    let mut acc = Accumulator::new(variant);
    for span in spans {
        for &byte in &data[span.start..span.end] {
            acc.update_byte(byte);
        }
    }
    Ok(Multi {
        base: acc.hashes(),
        fletcher16: acc.fletcher16(),
        fletcher32: acc.fletcher32(),
        crc16: acc.crc16(),
    })
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

    pub fn checksum(&self) -> Hashes {
        Hashes {
            crc32: self.crc ^ 0xffff_ffff,
            adler32: (self.b << 16) | self.a,
        }
    }

    pub fn from_seed(crc32: u32, adler32: u32) -> Result<Self, SnapshotError> {
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

    pub fn update(&mut self, data: &[u8]) {
        for &byte in data {
            self.crc = crc32_step(self.crc, byte);
            adler_step(&mut self.a, &mut self.b, byte);
        }
    }

    pub fn update_spans(
        &mut self,
        data: &[u8],
        spans: &[Span],
    ) -> Result<(), SpanError> {
        validate_spans(data.len(), spans)?;
        let mut next = self.clone();
        for span in spans {
            for &byte in &data[span.start..span.end] {
                next.crc = crc32_step(next.crc, byte);
                adler_step(&mut next.a, &mut next.b, byte);
            }
        }
        *self = next;
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
        for offset in 1..9 {
            crc = (crc << 4) | hex_value(bytes[offset]).ok_or(SnapshotError::Hex { offset })? as u32;
        }
        for offset in 11..19 {
            adler = (adler << 4)
                | hex_value(bytes[offset]).ok_or(SnapshotError::Hex { offset })? as u32;
        }
        Self::from_seed(crc, adler)
    }
}

impl Default for Stream {
    fn default() -> Self {
        Self::new()
    }
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}