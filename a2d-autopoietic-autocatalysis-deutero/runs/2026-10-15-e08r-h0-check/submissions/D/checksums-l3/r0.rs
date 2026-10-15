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
    const MOD: u32 = 65_521;
    for &byte in data {
        *a = (*a + byte as u32) % MOD;
        *b = (*b + *a) % MOD;
    }
}

pub fn crc32(data: &[u8]) -> u32 {
    crc32_update(0xffff_ffff, data) ^ 0xffff_ffff
}

pub fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1, 0);
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

fn update_fletcher16(a: &mut u16, b: &mut u16, data: &[u8]) {
    for &byte in data {
        *a = ((*a as u32 + byte as u32) % 255) as u16;
        *b = ((*b as u32 + *a as u32) % 255) as u16;
    }
}

fn update_fletcher32(a: &mut u32, b: &mut u32, word: u16) {
    *a = (*a + word as u32) % 65_535;
    *b = (*b + *a) % 65_535;
}

fn update_crc16(reg: &mut u16, data: &[u8], variant: Crc16) {
    match variant {
        Crc16::Xmodem | Crc16::False => {
            for &byte in data {
                *reg ^= (byte as u16) << 8;
                for _ in 0..8 {
                    let top = *reg & 0x8000;
                    *reg = reg.wrapping_shl(1);
                    if top != 0 {
                        *reg ^= 0x1021;
                    }
                }
            }
        }
        Crc16::X25 => {
            for &byte in data {
                *reg ^= byte as u16;
                for _ in 0..8 {
                    let low = *reg & 1;
                    *reg >>= 1;
                    if low != 0 {
                        *reg ^= 0x8408;
                    }
                }
            }
        }
    }
}

pub fn checksums_spans(data: &[u8], spans: &[Span]) -> Result<Hashes, SpanError> {
    validate_spans(data.len(), spans)?;
    let mut crc = 0xffff_ffff;
    let (mut a, mut b) = (1, 0);
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

pub fn fletcher16(data: &[u8]) -> u16 {
    let (mut a, mut b) = (0, 0);
    update_fletcher16(&mut a, &mut b, data);
    (b << 8) | a
}

pub fn fletcher32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (0, 0);
    let mut chunks = data.chunks_exact(2);
    for pair in &mut chunks {
        update_fletcher32(&mut a, &mut b, u16::from_be_bytes([pair[0], pair[1]]));
    }
    if let Some(&last) = chunks.remainder().first() {
        update_fletcher32(&mut a, &mut b, (last as u16) << 8);
    }
    (b << 16) | a
}

pub fn fletcher32_spans(data: &[u8], spans: &[Span]) -> Result<u32, SpanError> {
    validate_spans(data.len(), spans)?;
    let (mut a, mut b) = (0, 0);
    let mut high_byte: Option<u8> = None;

    for span in spans {
        for &byte in &data[span.start..span.end] {
            if let Some(high) = high_byte.take() {
                update_fletcher32(&mut a, &mut b, u16::from_be_bytes([high, byte]));
            } else {
                high_byte = Some(byte);
            }
        }
    }
    if let Some(high) = high_byte {
        update_fletcher32(&mut a, &mut b, (high as u16) << 8);
    }
    Ok((b << 16) | a)
}

pub fn crc16(data: &[u8], variant: Crc16) -> u16 {
    let mut reg = match variant {
        Crc16::Xmodem => 0,
        Crc16::False | Crc16::X25 => 0xffff,
    };
    update_crc16(&mut reg, data, variant);
    match variant {
        Crc16::X25 => reg ^ 0xffff,
        Crc16::Xmodem | Crc16::False => reg,
    }
}

pub fn multi_checksums(
    data: &[u8],
    spans: &[Span],
    variant: Crc16,
) -> Result<Multi, SpanError> {
    validate_spans(data.len(), spans)?;

    let mut crc = 0xffff_ffff;
    let (mut adler_a, mut adler_b) = (1, 0);
    let (mut f16_a, mut f16_b) = (0, 0);
    let (mut f32_a, mut f32_b) = (0, 0);
    let mut f32_high_byte: Option<u8> = None;
    let mut crc16_reg = match variant {
        Crc16::Xmodem => 0,
        Crc16::False | Crc16::X25 => 0xffff,
    };

    for span in spans {
        let selected = &data[span.start..span.end];
        crc = crc32_update(crc, selected);
        adler_update(&mut adler_a, &mut adler_b, selected);
        update_fletcher16(&mut f16_a, &mut f16_b, selected);
        update_crc16(&mut crc16_reg, selected, variant);

        for &byte in selected {
            if let Some(high) = f32_high_byte.take() {
                update_fletcher32(&mut f32_a, &mut f32_b, u16::from_be_bytes([high, byte]));
            } else {
                f32_high_byte = Some(byte);
            }
        }
    }

    if let Some(high) = f32_high_byte {
        update_fletcher32(&mut f32_a, &mut f32_b, (high as u16) << 8);
    }

    let crc16_result = match variant {
        Crc16::X25 => crc16_reg ^ 0xffff,
        Crc16::Xmodem | Crc16::False => crc16_reg,
    };

    Ok(Multi {
        base: Hashes {
            crc32: crc ^ 0xffff_ffff,
            adler32: (adler_b << 16) | adler_a,
        },
        fletcher16: (f16_b << 8) | f16_a,
        fletcher32: (f32_b << 16) | f32_a,
        crc16: crc16_result,
    })
}

#[derive(Debug, Clone)]
pub struct Stream {
    crc_state: u32,
    adler_a: u32,
    adler_b: u32,
}

impl Stream {
    pub fn new() -> Self {
        Self {
            crc_state: 0xffff_ffff,
            adler_a: 1,
            adler_b: 0,
        }
    }

    pub fn checksum(&self) -> Hashes {
        Hashes {
            crc32: self.crc_state ^ 0xffff_ffff,
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
            crc_state: crc32 ^ 0xffff_ffff,
            adler_a: a,
            adler_b: b,
        })
    }

    pub fn update(&mut self, data: &[u8]) {
        self.crc_state = crc32_update(self.crc_state, data);
        adler_update(&mut self.adler_a, &mut self.adler_b, data);
    }

    pub fn update_spans(&mut self, data: &[u8], spans: &[Span]) -> Result<(), SpanError> {
        validate_spans(data.len(), spans)?;
        for span in spans {
            let selected = &data[span.start..span.end];
            self.crc_state = crc32_update(self.crc_state, selected);
            adler_update(&mut self.adler_a, &mut self.adler_b, selected);
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