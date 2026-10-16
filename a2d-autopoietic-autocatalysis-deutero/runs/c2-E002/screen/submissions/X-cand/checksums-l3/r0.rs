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

fn adler_update(mut a: u32, mut b: u32, data: &[u8]) -> (u32, u32) {
    for &byte in data {
        a = (a + byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    (a, b)
}

pub fn crc32(data: &[u8]) -> u32 {
    crc32_update(0xffff_ffff, data) ^ 0xffff_ffff
}

pub fn adler32(data: &[u8]) -> u32 {
    let (a, b) = adler_update(1, 0, data);
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
    let mut a = 1;
    let mut b = 0;
    for span in spans {
        let selected = &data[span.start..span.end];
        crc = crc32_update(crc, selected);
        (a, b) = adler_update(a, b, selected);
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
        if a >= 65521 {
            return Err(SnapshotError::Adler { component: 0 });
        }
        if b >= 65521 {
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
            adler_update(self.adler_a, self.adler_b, data);
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
        let sums = self.checksum();
        format!("c{:08x}:a{:08x}", sums.crc32, sums.adler32)
    }

    pub fn restore(token: &str) -> Result<Self, SnapshotError> {
        let bytes = token.as_bytes();
        if bytes.len() != 19 || bytes[0] != b'c' || bytes[9] != b':' || bytes[10] != b'a' {
            return Err(SnapshotError::Shape);
        }

        let mut crc = 0u32;
        let mut adler = 0u32;
        for (offset, &byte) in bytes.iter().enumerate() {
            if !(1..=8).contains(&offset) && !(11..=18).contains(&offset) {
                continue;
            }
            let digit = match byte {
                b'0'..=b'9' => (byte - b'0') as u32,
                b'a'..=b'f' => (byte - b'a' + 10) as u32,
                _ => return Err(SnapshotError::Hex { offset }),
            };
            if offset <= 8 {
                crc = (crc << 4) | digit;
            } else {
                adler = (adler << 4) | digit;
            }
        }

        Self::from_seed(crc, adler)
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

fn fletcher32_update(
    mut a: u32,
    mut b: u32,
    data: &[u8],
    pending_high: &mut Option<u8>,
) -> (u32, u32) {
    for &byte in data {
        if let Some(high) = pending_high.take() {
            let word = ((high as u32) << 8) | byte as u32;
            a = (a + word) % 65535;
            b = (b + a) % 65535;
        } else {
            *pending_high = Some(byte);
        }
    }
    (a, b)
}

pub fn fletcher32(data: &[u8]) -> u32 {
    let mut a = 0;
    let mut b = 0;
    let mut pending = None;
    (a, b) = fletcher32_update(a, b, data, &mut pending);
    if let Some(high) = pending {
        a = (a + ((high as u32) << 8)) % 65535;
        b = (b + a) % 65535;
    }
    (b << 16) | a
}

pub fn fletcher32_spans(data: &[u8], spans: &[Span]) -> Result<u32, SpanError> {
    validate_spans(data.len(), spans)?;

    let mut a = 0;
    let mut b = 0;
    let mut pending = None;
    for span in spans {
        (a, b) = fletcher32_update(
            a,
            b,
            &data[span.start..span.end],
            &mut pending,
        );
    }
    if let Some(high) = pending {
        a = (a + ((high as u32) << 8)) % 65535;
        b = (b + a) % 65535;
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
    let mut c16 = match variant {
        Crc16::Xmodem => 0,
        Crc16::False | Crc16::X25 => 0xffff,
    };

    for span in spans {
        let selected = &data[span.start..span.end];
        crc = crc32_update(crc, selected);
        (adler_a, adler_b) = adler_update(adler_a, adler_b, selected);

        for &byte in selected {
            f16_a = (f16_a + byte as u32) % 255;
            f16_b = (f16_b + f16_a) % 255;
        }
        (f32_a, f32_b) =
            fletcher32_update(f32_a, f32_b, selected, &mut f32_pending);
        c16 = crc16_update(c16, selected, variant);
    }

    if let Some(high) = f32_pending {
        f32_a = (f32_a + ((high as u32) << 8)) % 65535;
        f32_b = (f32_b + f32_a) % 65535;
    }
    if variant == Crc16::X25 {
        c16 ^= 0xffff;
    }

    Ok(Multi {
        base: Hashes {
            crc32: crc ^ 0xffff_ffff,
            adler32: (adler_b << 16) | adler_a,
        },
        fletcher16: ((f16_b as u16) << 8) | f16_a as u16,
        fletcher32: (f32_b << 16) | f32_a,
        crc16: c16,
    })
}