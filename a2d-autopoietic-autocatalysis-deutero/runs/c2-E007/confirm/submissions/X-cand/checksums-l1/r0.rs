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

struct ChecksumState {
    crc: u32,
    a: u32,
    b: u32,
}

impl ChecksumState {
    fn new() -> Self {
        Self {
            crc: 0xffff_ffff,
            a: 1,
            b: 0,
        }
    }

    fn update(&mut self, data: &[u8]) {
        const ADLER_MOD: u32 = 65_521;

        for &byte in data {
            self.crc ^= u32::from(byte);
            for _ in 0..8 {
                let low_bit = self.crc & 1;
                self.crc >>= 1;
                if low_bit != 0 {
                    self.crc ^= 0xedb8_8320;
                }
            }

            self.a = (self.a + u32::from(byte)) % ADLER_MOD;
            self.b = (self.b + self.a) % ADLER_MOD;
        }
    }

    fn finish(self) -> Hashes {
        Hashes {
            crc32: self.crc ^ 0xffff_ffff,
            adler32: (self.b << 16) | self.a,
        }
    }
}

pub fn crc32(data: &[u8]) -> u32 {
    let mut state = ChecksumState::new();
    state.update(data);
    state.finish().crc32
}

pub fn adler32(data: &[u8]) -> u32 {
    let mut state = ChecksumState::new();
    state.update(data);
    state.finish().adler32
}

pub fn checksums(data: &[u8]) -> Hashes {
    let mut state = ChecksumState::new();
    state.update(data);
    state.finish()
}

pub fn checksums_spans(data: &[u8], spans: &[Span]) -> Result<Hashes, SpanError> {
    let mut state = ChecksumState::new();
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

        state.update(&data[span.start..span.end]);
        previous_end = span.end;
    }

    Ok(state.finish())
}