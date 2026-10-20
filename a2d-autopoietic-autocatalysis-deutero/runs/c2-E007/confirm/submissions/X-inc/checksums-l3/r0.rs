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

fn crc32_update(mut crc: u32, data: &[u8]) -> u32 {
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            let low = crc & 1;
            crc >>= 1;
            if low != 0 {
                crc ^= 0xedb8_8320;
            }
        }
    }
    crc
}

fn adler32_update(mut a: u32, mut b: u32, data: &[u8]) -> (u32, u32) {
    for &byte in data {
        a = (a + byte as u32) % 65_521;
        b = (b + a) % 65_521;
    }
    (a, b)
}

pub fn crc32(data: &[u8]) -> u32 {
    crc32_update(0xffff_ffff, data) ^ 0xffff_ffff
}

pub fn adler32(data: &[u8]) -> u32 {
    let (a, b) = adler32_update(1, 0, data);
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

    let mut crc = 0xffff_ffff;
    let (mut a, mut b) = (1u32, 0u32);
    for span in spans {
        crc = crc32_update(crc, &data[span.start..span.end]);
        (a, b) = adler32_update(a, b, &data[span.start..span.end]);
    }

    Ok(Hashes {
        crc32: crc ^ 0xffff_ffff,
        adler32: (b << 16) | a,
    })
}

#[derive(Debug, Clone)]
pub struct Stream {
    crc: u32,
    adler_a: u32,
    adler_b: u32,
}

impl Stream {
    pub fn new() -> Self {
        Self {
            crc: 0xffff_ffff,
            adler_a: 1,
            adler_b: 0,
        }
    }

    pub fn checksum(&self) -> Hashes {
        Hashes {
            crc32: self.crc ^ 0xffff_ffff,
            adler32: (self.adler_b << 16) | self.adler_a,
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
            adler_a: a,
            adler_b: b,
        })
    }

    pub fn update(&mut self, data: &[u8]) {
        self.crc = crc32_update(self.crc, data);
        (self.adler_a, self.adler_b) =
            adler32_update(self.adler_a, self.adler_b, data);
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
        let checksums = self.checksum();
        format!(
            "c{:08x}:a{:08x}",
            checksums.crc32, checksums.adler32
        )
    }

    pub fn restore(token: &str) -> Result<Self, SnapshotError> {
        let bytes = token.as_bytes();
        if bytes.len() != 19 || bytes[0] != b'c' || bytes[9] != b':' || bytes[10] != b'a' {
            return Err(SnapshotError::Shape);
        }

        let mut crc = 0u32;
        let mut adler = 0u32;
        for offset in 1..9 {
            let digit = hex_value(bytes[offset])
                .ok_or(SnapshotError::Hex { offset })?;
            crc = (crc << 4) | digit as u32;
        }
        for offset in 11..19 {
            let digit = hex_value(bytes[offset])
                .ok_or(SnapshotError::Hex { offset })?;
            adler = (adler << 4) | digit as u32;
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

pub fn fletcher16(data: &[u8]) -> u16 {
    let (mut a, mut b) = (0u32, 0u32);
    for &byte in data {
        a = (a + byte as u32) % 255;
        b = (b + a) % 255;
    }
    ((b as u16) << 8) | a as u16
}

fn fletcher32_word(mut a: u32, mut b: u32, word: u16) -> (u32, u32) {
    a = (a + word as u32) % 65_535;
    b = (b + a) % 65_535;
    (a, b)
}

pub fn fletcher32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (0u32, 0u32);
    let mut chunks = data.chunks_exact(2);
    for pair in &mut chunks {
        let word = u16::from_be_bytes([pair[0], pair[1]]);
        (a, b) = fletcher32_word(a, b, word);
    }
    if let Some(&last) = chunks.remainder().first() {
        (a, b) = fletcher32_word(a, b, (last as u16) << 8);
    }
    (b << 16) | a
}

pub fn fletcher32_spans(data: &[u8], spans: &[Span]) -> Result<u32, SpanError> {
    validate_spans(data.len(), spans)?;

    let (mut a, mut b) = (0u32, 0u32);
    let mut pending_high: Option<u8> = None;

    for span in spans {
        for &byte in &data[span.start..span.end] {
            if let Some(high) = pending_high.take() {
                (a, b) = fletcher32_word(a, b, u16::from_be_bytes([high, byte]));
            } else {
                pending_high = Some(byte);
            }
        }
    }

    if let Some(high) = pending_high {
        (a, b) = fletcher32_word(a, b, (high as u16) << 8);
    }

    Ok((b << 16) | a)
}

fn crc16_update(mut register: u16, data: &[u8], variant: Crc16) -> u16 {
    for &byte in data {
        match variant {
            Crc16::Xmodem | Crc16::False => {
                register ^= (byte as u16) << 8;
                for _ in 0..8 {
                    let top = register & 0x8000;
                    register = register.wrapping_shl(1);
                    if top != 0 {
                        register ^= 0x1021;
                    }
                }
            }
            Crc16::X25 => {
                register ^= byte as u16;
                for _ in 0..8 {
                    let low = register & 1;
                    register >>= 1;
                    if low != 0 {
                        register ^= 0x8408;
                    }
                }
            }
        }
    }
    register
}

pub fn crc16(data: &[u8], variant: Crc16) -> u16 {
    let initial = match variant {
        Crc16::Xmodem => 0,
        Crc16::False | Crc16::X25 => 0xffff,
    };
    let register = crc16_update(initial, data, variant);
    match variant {
        Crc16::X25 => register ^ 0xffff,
        Crc16::Xmodem | Crc16::False => register,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Multi {
    pub base: Hashes,
    pub fletcher16: u16,
    pub fletcher32: u32,
    pub crc16: u16,
}

pub fn multi_checksums(
    data: &[u8],
    spans: &[Span],
    variant: Crc16,
) -> Result<Multi, SpanError> {
    validate_spans(data.len(), spans)?;

    let mut crc = 0xffff_ffff;
    let (mut adler_a, mut adler_b) = (1u32, 0u32);
    let (mut f16_a, mut f16_b) = (0u32, 0u32);
    let (mut f32_a, mut f32_b) = (0u32, 0u32);
    let mut pending_high: Option<u8> = None;

    let initial_crc16 = match variant {
        Crc16::Xmodem => 0,
        Crc16::False | Crc16::X25 => 0xffff,
    };
    let mut crc16_register = initial_crc16;

    for span in spans {
        let selected = &data[span.start..span.end];

        crc = crc32_update(crc, selected);
        (adler_a, adler_b) = adler32_update(adler_a, adler_b, selected);
        crc16_register = crc16_update(crc16_register, selected, variant);

        for &byte in selected {
            f16_a = (f16_a + byte as u32) % 255;
            f16_b = (f16_b + f16_a) % 255;

            if let Some(high) = pending_high.take() {
                (f32_a, f32_b) =
                    fletcher32_word(f32_a, f32_b, u16::from_be_bytes([high, byte]));
            } else {
                pending_high = Some(byte);
            }
        }
    }

    if let Some(high) = pending_high {
        (f32_a, f32_b) = fletcher32_word(f32_a, f32_b, (high as u16) << 8);
    }

    if variant == Crc16::X25 {
        crc16_register ^= 0xffff;
    }

    Ok(Multi {
        base: Hashes {
            crc32: crc ^ 0xffff_ffff,
            adler32: (adler_b << 16) | adler_a,
        },
        fletcher16: ((f16_b as u16) << 8) | f16_a as u16,
        fletcher32: (f32_b << 16) | f32_a,
        crc16: crc16_register,
    })
}