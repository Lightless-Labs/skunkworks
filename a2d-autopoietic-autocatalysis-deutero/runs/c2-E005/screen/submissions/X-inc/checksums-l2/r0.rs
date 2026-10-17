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

fn crc32_update(mut crc: u32, data: &[u8]) -> u32 {
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
    crc
}

fn adler32_update(mut a: u32, mut b: u32, data: &[u8]) -> (u32, u32) {
    for &byte in data {
        a = (a + u32::from(byte)) % 65_521;
        b = (b + a) % 65_521;
    }
    (a, b)
}

fn finalized(crc: u32, a: u32, b: u32) -> Hashes {
    Hashes {
        crc32: crc ^ 0xffff_ffff,
        adler32: (b << 16) | a,
    }
}

pub fn crc32(data: &[u8]) -> u32 {
    crc32_update(0xffff_ffff, data) ^ 0xffff_ffff
}

pub fn adler32(data: &[u8]) -> u32 {
    let (a, b) = adler32_update(1, 0, data);
    (b << 16) | a
}

pub fn checksums(data: &[u8]) -> Hashes {
    let crc = crc32_update(0xffff_ffff, data);
    let (a, b) = adler32_update(1, 0, data);
    finalized(crc, a, b)
}

fn validate_spans(data_len: usize, spans: &[Span]) -> Result<(), SpanError> {
    let mut preceding_end = 0;

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
        if index > 0 && span.start < preceding_end {
            return Err(SpanError::Overlap { span: index });
        }
        preceding_end = span.end;
    }

    Ok(())
}

fn update_spans_state(
    crc: &mut u32,
    a: &mut u32,
    b: &mut u32,
    data: &[u8],
    spans: &[Span],
) {
    for span in spans {
        *crc = crc32_update(*crc, &data[span.start..span.end]);
        (*a, *b) = adler32_update(*a, *b, &data[span.start..span.end]);
    }
}

pub fn checksums_spans(data: &[u8], spans: &[Span]) -> Result<Hashes, SpanError> {
    validate_spans(data.len(), spans)?;

    let mut crc = 0xffff_ffff;
    let mut a = 1;
    let mut b = 0;
    update_spans_state(&mut crc, &mut a, &mut b, data, spans);

    Ok(finalized(crc, a, b))
}

#[derive(Debug, Clone)]
pub struct Stream {
    crc: u32,
    a: u32,
    b: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotError {
    Shape,
    Hex { offset: usize },
    Adler { component: usize },
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
        finalized(self.crc, self.a, self.b)
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

    pub fn update(&mut self, data: &[u8]) {
        self.crc = crc32_update(self.crc, data);
        (self.a, self.b) = adler32_update(self.a, self.b, data);
    }

    pub fn update_spans(&mut self, data: &[u8], spans: &[Span]) -> Result<(), SpanError> {
        validate_spans(data.len(), spans)?;

        let mut crc = self.crc;
        let mut a = self.a;
        let mut b = self.b;
        update_spans_state(&mut crc, &mut a, &mut b, data, spans);

        self.crc = crc;
        self.a = a;
        self.b = b;
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

        fn hex_value(byte: u8, offset: usize) -> Result<u32, SnapshotError> {
            match byte {
                b'0'..=b'9' => Ok(u32::from(byte - b'0')),
                b'a'..=b'f' => Ok(u32::from(byte - b'a' + 10)),
                _ => Err(SnapshotError::Hex { offset }),
            }
        }

        let mut crc = 0u32;
        for offset in 1..9 {
            crc = (crc << 4) | hex_value(bytes[offset], offset)?;
        }

        let mut adler = 0u32;
        for offset in 11..19 {
            adler = (adler << 4) | hex_value(bytes[offset], offset)?;
        }

        Self::from_seed(crc, adler)
    }
}

impl Default for Stream {
    fn default() -> Self {
        Self::new()
    }
}