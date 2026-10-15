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

fn crc32_update(mut crc: u32, data: &[u8]) -> u32 {
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xedb88320;
            } else {
                crc >>= 1;
            }
        }
    }
    crc
}

fn adler32_update(mut a: u32, mut b: u32, data: &[u8]) -> (u32, u32) {
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

pub fn checksums_spans(data: &[u8], spans: &[Span]) -> Result<Hashes, SpanError> {
    validate_spans(data.len(), spans)?;

    let mut crc = 0xffff_ffff;
    let mut a = 1;
    let mut b = 0;
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
        let hashes = self.checksum();
        format!("c{:08x}:a{:08x}", hashes.crc32, hashes.adler32)
    }

    pub fn restore(token: &str) -> Result<Self, SnapshotError> {
        let bytes = token.as_bytes();
        if bytes.len() != 19
            || bytes[0] != b'c'
            || bytes[9] != b':'
            || bytes[10] != b'a'
        {
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

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

fn main() {}