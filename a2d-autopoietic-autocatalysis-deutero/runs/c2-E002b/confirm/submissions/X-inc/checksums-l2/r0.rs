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
    adler_a: u32,
    adler_b: u32,
}

fn update_crc(crc: &mut u32, data: &[u8]) {
    for &byte in data {
        *crc ^= u32::from(byte);
        for _ in 0..8 {
            let low_bit = *crc & 1;
            *crc >>= 1;
            if low_bit != 0 {
                *crc ^= 0xedb8_8320;
            }
        }
    }
}

fn update_adler(a: &mut u32, b: &mut u32, data: &[u8]) {
    const MOD: u32 = 65_521;
    for &byte in data {
        *a = (*a + u32::from(byte)) % MOD;
        *b = (*b + *a) % MOD;
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

impl Stream {
    pub fn new() -> Self {
        Self {
            crc: 0xffff_ffff,
            adler_a: 1,
            adler_b: 0,
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

    pub fn checksum(&self) -> Hashes {
        Hashes {
            crc32: self.crc ^ 0xffff_ffff,
            adler32: (self.adler_b << 16) | self.adler_a,
        }
    }

    pub fn update(&mut self, data: &[u8]) {
        update_crc(&mut self.crc, data);
        update_adler(&mut self.adler_a, &mut self.adler_b, data);
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
        for offset in 1..9 {
            crc = (crc << 4)
                | u32::from(hex_digit(bytes[offset]).ok_or(SnapshotError::Hex { offset })?);
        }

        let mut adler = 0u32;
        for offset in 11..19 {
            adler = (adler << 4)
                | u32::from(hex_digit(bytes[offset]).ok_or(SnapshotError::Hex { offset })?);
        }

        Self::from_seed(crc, adler)
    }
}

impl Default for Stream {
    fn default() -> Self {
        Self::new()
    }
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffff;
    update_crc(&mut crc, data);
    crc ^ 0xffff_ffff
}

pub fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1, 0);
    update_adler(&mut a, &mut b, data);
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