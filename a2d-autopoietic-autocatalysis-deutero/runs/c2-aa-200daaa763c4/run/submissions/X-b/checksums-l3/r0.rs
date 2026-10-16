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

#[derive(Debug, Clone)]
pub struct Stream {
    crc: u32,
    a: u32,
    b: u32,
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

fn adler_update(a: &mut u32, b: &mut u32, data: &[u8]) {
    for &byte in data {
        *a = (*a + byte as u32) % ADLER_MOD;
        *b = (*b + *a) % ADLER_MOD;
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
    let mut previous_end = 0;
    for (i, span) in spans.iter().enumerate() {
        if span.start > span.end {
            return Err(SpanError::Reversed { span: i });
        }
        if span.start > data_len {
            return Err(SpanError::OutOfBounds {
                span: i,
                offset: span.start,
            });
        }
        if span.end > data_len {
            return Err(SpanError::OutOfBounds {
                span: i,
                offset: span.end,
            });
        }
        if i > 0 && span.start < previous_end {
            return Err(SpanError::Overlap { span: i });
        }
        previous_end = span.end;
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

        let mut values = [0u32; 2];
        for (index, range) in [(0, 1..9), (1, 11..19)] {
            let mut value = 0u32;
            for offset in range {
                let digit = match bytes[offset] {
                    b'0'..=b'9' => bytes[offset] - b'0',
                    b'a'..=b'f' => bytes[offset] - b'a' + 10,
                    _ => return Err(SnapshotError::Hex { offset }),
                };
                value = (value << 4) | digit as u32;
            }
            values[index] = value;
        }

        Self::from_seed(values[0], values[1])
    }
}

impl Default for Stream {
    fn default() -> Self {
        Self::new()
    }
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

fn fletcher32_word(a: &mut u32, b: &mut u32, word: u16) {
    *a = (*a + word as u32) % 65_535;
    *b = (*b + *a) % 65_535;
}

pub fn fletcher32(data: &[u8]) -> u32 {
    let mut a = 0u32;
    let mut b = 0u32;
    let mut chunks = data.chunks_exact(2);
    for pair in &mut chunks {
        fletcher32_word(&mut a, &mut b, u16::from_be_bytes([pair[0], pair[1]]));
    }
    if let Some(&last) = chunks.remainder().first() {
        fletcher32_word(&mut a, &mut b, (last as u16) << 8);
    }
    (b << 16) | a
}

pub fn fletcher32_spans(data: &[u8], spans: &[Span]) -> Result<u32, SpanError> {
    validate_spans(data.len(), spans)?;
    let mut a = 0u32;
    let mut b = 0u32;
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

fn crc16_update(mut reg: u16, data: &[u8], variant: Crc16) -> u16 {
    match variant {
        Crc16::Xmodem | Crc16::False => {
            for &byte in data {
                reg ^= (byte as u16) << 8;
                for _ in 0..8 {
                    let top = reg & 0x8000;
                    reg = reg.wrapping_shl(1);
                    if top != 0 {
                        reg ^= 0x1021;
                    }
                }
            }
            reg
        }
        Crc16::X25 => {
            for &byte in data {
                reg ^= byte as u16;
                for _ in 0..8 {
                    let low = reg & 1;
                    reg >>= 1;
                    if low != 0 {
                        reg ^= 0x8408;
                    }
                }
            }
            reg
        }
    }
}

pub fn crc16(data: &[u8], variant: Crc16) -> u16 {
    let initial = match variant {
        Crc16::Xmodem => 0,
        Crc16::False | Crc16::X25 => 0xffff,
    };
    let reg = crc16_update(initial, data, variant);
    if variant == Crc16::X25 {
        reg ^ 0xffff
    } else {
        reg
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
    let mut f16_a = 0u32;
    let mut f16_b = 0u32;
    let mut f32_a = 0u32;
    let mut f32_b = 0u32;
    let mut f32_pending = None;
    let crc16_initial = match variant {
        Crc16::Xmodem => 0,
        Crc16::False | Crc16::X25 => 0xffff,
    };
    let mut crc16_reg = crc16_initial;

    for span in spans {
        let selected = &data[span.start..span.end];
        crc = crc32_update(crc, selected);
        adler_update(&mut adler_a, &mut adler_b, selected);
        crc16_reg = crc16_update(crc16_reg, selected, variant);

        for &byte in selected {
            f16_a = (f16_a + byte as u32) % 255;
            f16_b = (f16_b + f16_a) % 255;

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
    if variant == Crc16::X25 {
        crc16_reg ^= 0xffff;
    }

    Ok(Multi {
        base: Hashes {
            crc32: crc ^ 0xffff_ffff,
            adler32: (adler_b << 16) | adler_a,
        },
        fletcher16: ((f16_b as u16) << 8) | f16_a as u16,
        fletcher32: (f32_b << 16) | f32_a,
        crc16: crc16_reg,
    })
}