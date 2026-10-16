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
            self.update_byte(byte);
        }
    }

    pub fn update_spans(
        &mut self,
        data: &[u8],
        spans: &[Span],
    ) -> Result<(), SpanError> {
        validate_spans(data.len(), spans)?;
        for span in spans {
            self.update(&data[span.start..span.end]);
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
        for (offset, &byte) in bytes.iter().enumerate() {
            if (1..9).contains(&offset) {
                crc = (crc << 4) | u32::from(hex_value(byte, offset)?);
            } else if (11..19).contains(&offset) {
                adler = (adler << 4) | u32::from(hex_value(byte, offset)?);
            }
        }
        Self::from_seed(crc, adler)
    }

    fn update_byte(&mut self, byte: u8) {
        self.crc ^= u32::from(byte);
        for _ in 0..8 {
            if self.crc & 1 != 0 {
                self.crc = (self.crc >> 1) ^ 0xedb8_8320;
            } else {
                self.crc >>= 1;
            }
        }

        self.a = (self.a + u32::from(byte)) % 65_521;
        self.b = (self.b + self.a) % 65_521;
    }
}

impl Default for Stream {
    fn default() -> Self {
        Self::new()
    }
}

fn hex_value(byte: u8, offset: usize) -> Result<u8, SnapshotError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        _ => Err(SnapshotError::Hex { offset }),
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
    let mut crc = 0xffff_ffffu32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xedb8_8320;
            } else {
                crc >>= 1;
            }
        }
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

pub fn checksums_spans(data: &[u8], spans: &[Span]) -> Result<Hashes, SpanError> {
    validate_spans(data.len(), spans)?;
    let mut stream = Stream::new();
    for span in spans {
        stream.update(&data[span.start..span.end]);
    }
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

pub fn fletcher32(data: &[u8]) -> u32 {
    let mut state = Fletcher32::new();
    for &byte in data {
        state.push_byte(byte);
    }
    state.finish()
}

struct Fletcher32 {
    a: u32,
    b: u32,
    pending: Option<u8>,
}

impl Fletcher32 {
    fn new() -> Self {
        Self {
            a: 0,
            b: 0,
            pending: None,
        }
    }

    fn push_byte(&mut self, byte: u8) {
        if let Some(high) = self.pending.take() {
            self.push_word((u16::from(high) << 8) | u16::from(byte));
        } else {
            self.pending = Some(byte);
        }
    }

    fn push_word(&mut self, word: u16) {
        self.a = (self.a + u32::from(word)) % 65_535;
        self.b = (self.b + self.a) % 65_535;
    }

    fn finish(mut self) -> u32 {
        if let Some(high) = self.pending {
            self.push_word(u16::from(high) << 8);
        }
        (self.b << 16) | self.a
    }
}

pub fn fletcher32_spans(data: &[u8], spans: &[Span]) -> Result<u32, SpanError> {
    validate_spans(data.len(), spans)?;
    let mut state = Fletcher32::new();
    for span in spans {
        for &byte in &data[span.start..span.end] {
            state.push_byte(byte);
        }
    }
    Ok(state.finish())
}

fn crc16_update(mut crc: u16, data: &[u8], variant: Crc16) -> u16 {
    match variant {
        Crc16::Xmodem | Crc16::False => {
            for &byte in data {
                crc ^= u16::from(byte) << 8;
                for _ in 0..8 {
                    crc = if crc & 0x8000 != 0 {
                        (crc << 1) ^ 0x1021
                    } else {
                        crc << 1
                    };
                }
            }
            crc
        }
        Crc16::X25 => {
            for &byte in data {
                crc ^= u16::from(byte);
                for _ in 0..8 {
                    crc = if crc & 1 != 0 {
                        (crc >> 1) ^ 0x8408
                    } else {
                        crc >> 1
                    };
                }
            }
            crc
        }
    }
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

    let mut base = Stream::new();
    let mut f16_a = 0u32;
    let mut f16_b = 0u32;
    let mut f32 = Fletcher32::new();
    let initial_crc16 = match variant {
        Crc16::Xmodem => 0,
        Crc16::False | Crc16::X25 => 0xffff,
    };
    let mut c16 = initial_crc16;

    for span in spans {
        let selected = &data[span.start..span.end];
        base.update(selected);

        for &byte in selected {
            f16_a = (f16_a + u32::from(byte)) % 255;
            f16_b = (f16_b + f16_a) % 255;
            f32.push_byte(byte);
        }

        c16 = crc16_update(c16, selected, variant);
    }

    if variant == Crc16::X25 {
        c16 ^= 0xffff;
    }

    Ok(Multi {
        base: base.checksum(),
        fletcher16: ((f16_b as u16) << 8) | f16_a as u16,
        fletcher32: f32.finish(),
        crc16: c16,
    })
}