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

fn crc32_update(mut state: u32, data: &[u8]) -> u32 {
    for &byte in data {
        state ^= byte as u32;
        for _ in 0..8 {
            let low = state & 1;
            state >>= 1;
            if low != 0 {
                state ^= 0xedb88320;
            }
        }
    }
    state
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

fn validate_spans(data: &[u8], spans: &[Span]) -> Result<(), SpanError> {
    let mut previous_end = 0;
    for (index, span) in spans.iter().enumerate() {
        if span.start > span.end {
            return Err(SpanError::Reversed { span: index });
        }
        if span.start > data.len() {
            return Err(SpanError::OutOfBounds {
                span: index,
                offset: span.start,
            });
        }
        if span.end > data.len() {
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
    validate_spans(data, spans)?;
    let mut crc = 0xffff_ffff;
    let (mut a, mut b) = (1, 0);
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

pub struct Stream {
    crc_state: u32,
    adler_a: u32,
    adler_b: u32,
}

impl std::fmt::Debug for Stream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Stream")
            .field("crc_state", &self.crc_state)
            .field("adler_a", &self.adler_a)
            .field("adler_b", &self.adler_b)
            .finish()
    }
}

impl Clone for Stream {
    fn clone(&self) -> Self {
        Self {
            crc_state: self.crc_state,
            adler_a: self.adler_a,
            adler_b: self.adler_b,
        }
    }
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
        if a >= 65521 {
            return Err(SnapshotError::Adler { component: 0 });
        }
        if b >= 65521 {
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
        (self.adler_a, self.adler_b) =
            adler_update(self.adler_a, self.adler_b, data);
    }

    pub fn update_spans(
        &mut self,
        data: &[u8],
        spans: &[Span],
    ) -> Result<(), SpanError> {
        validate_spans(data, spans)?;
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
        for offset in 1..9 {
            crc = (crc << 4)
                | hex_digit(bytes[offset]).ok_or(SnapshotError::Hex { offset })? as u32;
        }
        for offset in 11..19 {
            adler = (adler << 4)
                | hex_digit(bytes[offset]).ok_or(SnapshotError::Hex { offset })? as u32;
        }
        Self::from_seed(crc, adler)
    }
}

fn hex_digit(byte: u8) -> Option<u8> {
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

fn fletcher32_update(mut a: u32, mut b: u32, mut pending: Option<u8>, data: &[u8]) -> (u32, u32, Option<u8>) {
    let mut index = 0;
    if let Some(high) = pending.take() {
        if let Some(&low) = data.first() {
            let word = ((high as u32) << 8) | low as u32;
            a = (a + word) % 65535;
            b = (b + a) % 65535;
            index = 1;
        } else {
            return (a, b, Some(high));
        }
    }

    while index + 1 < data.len() {
        let word = ((data[index] as u32) << 8) | data[index + 1] as u32;
        a = (a + word) % 65535;
        b = (b + a) % 65535;
        index += 2;
    }

    let pending = if index < data.len() {
        Some(data[index])
    } else {
        None
    };
    (a, b, pending)
}

pub fn fletcher32(data: &[u8]) -> u32 {
    let (mut a, mut b, pending) = fletcher32_update(0, 0, None, data);
    if let Some(high) = pending {
        let word = (high as u32) << 8;
        a = (a + word) % 65535;
        b = (b + a) % 65535;
    }
    (b << 16) | a
}

pub fn fletcher32_spans(data: &[u8], spans: &[Span]) -> Result<u32, SpanError> {
    validate_spans(data, spans)?;
    let (mut a, mut b, mut pending) = (0, 0, None);
    for span in spans {
        (a, b, pending) =
            fletcher32_update(a, b, pending, &data[span.start..span.end]);
    }
    if let Some(high) = pending {
        a = (a + ((high as u32) << 8)) % 65535;
        b = (b + a) % 65535;
    }
    Ok((b << 16) | a)
}

fn crc16_update(mut state: u16, data: &[u8], variant: Crc16) -> u16 {
    match variant {
        Crc16::Xmodem | Crc16::False => {
            for &byte in data {
                state ^= (byte as u16) << 8;
                for _ in 0..8 {
                    let top = state & 0x8000;
                    state <<= 1;
                    if top != 0 {
                        state ^= 0x1021;
                    }
                }
            }
        }
        Crc16::X25 => {
            for &byte in data {
                state ^= byte as u16;
                for _ in 0..8 {
                    let low = state & 1;
                    state >>= 1;
                    if low != 0 {
                        state ^= 0x8408;
                    }
                }
            }
        }
    }
    state
}

pub fn crc16(data: &[u8], variant: Crc16) -> u16 {
    let initial = match variant {
        Crc16::Xmodem => 0,
        Crc16::False | Crc16::X25 => 0xffff,
    };
    let result = crc16_update(initial, data, variant);
    if variant == Crc16::X25 {
        result ^ 0xffff
    } else {
        result
    }
}

pub fn multi_checksums(
    data: &[u8],
    spans: &[Span],
    variant: Crc16,
) -> Result<Multi, SpanError> {
    validate_spans(data, spans)?;

    let (mut crc, mut adler_a, mut adler_b) = (0xffff_ffff, 1, 0);
    let (mut f16_a, mut f16_b) = (0u32, 0u32);
    let (mut f32_a, mut f32_b, mut f32_pending) = (0u32, 0u32, None);
    let crc16_initial = match variant {
        Crc16::Xmodem => 0,
        Crc16::False | Crc16::X25 => 0xffff,
    };
    let mut crc16_state = crc16_initial;

    for span in spans {
        let selected = &data[span.start..span.end];
        crc = crc32_update(crc, selected);
        (adler_a, adler_b) = adler_update(adler_a, adler_b, selected);
        for &byte in selected {
            f16_a = (f16_a + byte as u32) % 255;
            f16_b = (f16_b + f16_a) % 255;
        }
        (f32_a, f32_b, f32_pending) =
            fletcher32_update(f32_a, f32_b, f32_pending, selected);
        crc16_state = crc16_update(crc16_state, selected, variant);
    }

    if let Some(high) = f32_pending {
        f32_a = (f32_a + ((high as u32) << 8)) % 65535;
        f32_b = (f32_b + f32_a) % 65535;
    }
    if variant == Crc16::X25 {
        crc16_state ^= 0xffff;
    }

    Ok(Multi {
        base: Hashes {
            crc32: crc ^ 0xffff_ffff,
            adler32: (adler_b << 16) | adler_a,
        },
        fletcher16: ((f16_b as u16) << 8) | f16_a as u16,
        fletcher32: (f32_b << 16) | f32_a,
        crc16: crc16_state,
    })
}

fn main() {}