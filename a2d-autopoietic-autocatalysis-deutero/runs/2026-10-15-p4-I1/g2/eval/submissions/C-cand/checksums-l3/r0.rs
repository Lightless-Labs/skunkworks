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

fn crc32_update(mut crc: u32, data: &[u8]) -> u32 {
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            let low = crc & 1;
            crc >>= 1;
            if low != 0 {
                crc ^= 0xedb88320;
            }
        }
    }
    crc
}

fn adler_update(a: &mut u32, b: &mut u32, data: &[u8]) {
    for &byte in data {
        *a = (*a + byte as u32) % 65521;
        *b = (*b + *a) % 65521;
    }
}

pub fn crc32(data: &[u8]) -> u32 {
    crc32_update(0xffff_ffff, data) ^ 0xffff_ffff
}

pub fn adler32(data: &[u8]) -> u32 {
    let mut a = 1;
    let mut b = 0;
    adler_update(&mut a, &mut b, data);
    (b << 16) | a
}

pub fn checksums(data: &[u8]) -> Hashes {
    Hashes {
        crc32: crc32(data),
        adler32: adler32(data),
    }
}

fn validate_spans(data_len: usize, spans: &[Span]) -> Result<(), SpanError> {
    let mut previous_end = None;
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
        if let Some(end) = previous_end {
            if span.start < end {
                return Err(SpanError::Overlap { span: index });
            }
        }
        previous_end = Some(span.end);
    }
    Ok(())
}

pub fn checksums_spans(data: &[u8], spans: &[Span]) -> Result<Hashes, SpanError> {
    validate_spans(data.len(), spans)?;
    let mut crc = 0xffff_ffff;
    let mut a = 1;
    let mut b = 0;
    for span in spans {
        let selected = &data[span.start..span.end];
        crc = crc32_update(crc, selected);
        adler_update(&mut a, &mut b, selected);
    }
    Ok(Hashes {
        crc32: crc ^ 0xffff_ffff,
        adler32: (b << 16) | a,
    })
}

fn fletcher16_update(a: &mut u32, b: &mut u32, data: &[u8]) {
    for &byte in data {
        *a = (*a + byte as u32) % 255;
        *b = (*b + *a) % 255;
    }
}

pub fn fletcher16(data: &[u8]) -> u16 {
    let mut a = 0;
    let mut b = 0;
    fletcher16_update(&mut a, &mut b, data);
    ((b as u16) << 8) | a as u16
}

fn fletcher32_word(a: &mut u32, b: &mut u32, word: u16) {
    *a = (*a + word as u32) % 65535;
    *b = (*b + *a) % 65535;
}

pub fn fletcher32(data: &[u8]) -> u32 {
    let mut a = 0;
    let mut b = 0;
    let mut chunks = data.chunks_exact(2);
    for chunk in &mut chunks {
        fletcher32_word(&mut a, &mut b, u16::from_be_bytes([chunk[0], chunk[1]]));
    }
    if let Some(&last) = chunks.remainder().first() {
        fletcher32_word(&mut a, &mut b, (last as u16) << 8);
    }
    (b << 16) | a
}

pub fn fletcher32_spans(data: &[u8], spans: &[Span]) -> Result<u32, SpanError> {
    validate_spans(data.len(), spans)?;
    let mut a = 0;
    let mut b = 0;
    let mut pending_high = None;
    for span in spans {
        for &byte in &data[span.start..span.end] {
            if let Some(high) = pending_high.take() {
                fletcher32_word(&mut a, &mut b, ((high as u16) << 8) | byte as u16);
            } else {
                pending_high = Some(byte);
            }
        }
    }
    if let Some(high) = pending_high {
        fletcher32_word(&mut a, &mut b, (high as u16) << 8);
    }
    Ok((b << 16) | a)
}

fn crc16_update(mut crc: u16, data: &[u8], variant: Crc16) -> u16 {
    match variant {
        Crc16::Xmodem | Crc16::False => {
            for &byte in data {
                crc ^= (byte as u16) << 8;
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
                crc ^= byte as u16;
                for _ in 0..8 {
                    let low = crc & 1;
                    crc >>= 1;
                    if low != 0 {
                        crc ^= 0x8408;
                    }
                }
            }
            crc
        }
    }
}

pub fn crc16(data: &[u8], variant: Crc16) -> u16 {
    match variant {
        Crc16::Xmodem => crc16_update(0, data, variant),
        Crc16::False => crc16_update(0xffff, data, variant),
        Crc16::X25 => crc16_update(0xffff, data, variant) ^ 0xffff,
    }
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
        if a >= 65521 {
            return Err(SnapshotError::Adler { component: 0 });
        }
        if b >= 65521 {
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
        self.crc = crc32_update(self.crc, data);
        adler_update(&mut self.a, &mut self.b, data);
    }

    pub fn update_spans(&mut self, data: &[u8], spans: &[Span]) -> Result<(), SpanError> {
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
        for offset in 1..9 {
            crc = (crc << 4) | hex_value(bytes[offset], offset)? as u32;
        }
        let mut adler = 0u32;
        for offset in 11..19 {
            adler = (adler << 4) | hex_value(bytes[offset], offset)? as u32;
        }
        Self::from_seed(crc, adler)
    }
}

fn hex_value(byte: u8, offset: usize) -> Result<u8, SnapshotError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        _ => Err(SnapshotError::Hex { offset }),
    }
}

pub fn multi_checksums(
    data: &[u8],
    spans: &[Span],
    variant: Crc16,
) -> Result<Multi, SpanError> {
    validate_spans(data.len(), spans)?;

    let mut crc = 0xffff_ffff;
    let mut adler_a = 1;
    let mut adler_b = 0;
    let mut f16_a = 0;
    let mut f16_b = 0;
    let mut f32_a = 0;
    let mut f32_b = 0;
    let mut f32_pending = None;

    let mut crc16_state = match variant {
        Crc16::Xmodem => 0,
        Crc16::False | Crc16::X25 => 0xffff,
    };

    for span in spans {
        let selected = &data[span.start..span.end];
        crc = crc32_update(crc, selected);
        adler_update(&mut adler_a, &mut adler_b, selected);
        fletcher16_update(&mut f16_a, &mut f16_b, selected);
        crc16_state = crc16_update(crc16_state, selected, variant);

        for &byte in selected {
            if let Some(high) = f32_pending.take() {
                fletcher32_word(&mut f32_a, &mut f32_b, ((high as u16) << 8) | byte as u16);
            } else {
                f32_pending = Some(byte);
            }
        }
    }

    if let Some(high) = f32_pending {
        fletcher32_word(&mut f32_a, &mut f32_b, (high as u16) << 8);
    }

    let crc16_result = if variant == Crc16::X25 {
        crc16_state ^ 0xffff
    } else {
        crc16_state
    };

    Ok(Multi {
        base: Hashes {
            crc32: crc ^ 0xffff_ffff,
            adler32: (adler_b << 16) | adler_a,
        },
        fletcher16: ((f16_b as u16) << 8) | f16_a as u16,
        fletcher32: (f32_b << 16) | f32_a,
        crc16: crc16_result,
    })
}

fn main() {}