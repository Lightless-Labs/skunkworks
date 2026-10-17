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

pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;

    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let low_bit = crc & 1;
            crc >>= 1;
            if low_bit != 0 {
                crc ^= 0xedb8_8320;
            }
        }
    }

    crc ^ 0xffff_ffff
}

pub fn adler32(data: &[u8]) -> u32 {
    const MOD_ADLER: u64 = 65_521;

    let mut a = 1u64;
    let mut b = 0u64;

    for &byte in data {
        a = (a + u64::from(byte)) % MOD_ADLER;
        b = (b + a) % MOD_ADLER;
    }

    ((b as u32) << 16) | (a as u32)
}

pub fn checksums(data: &[u8]) -> Hashes {
    Hashes {
        crc32: crc32(data),
        adler32: adler32(data),
    }
}

pub fn checksums_spans(data: &[u8], spans: &[Span]) -> Result<Hashes, SpanError> {
    let mut crc = 0xffff_ffffu32;
    let mut a = 1u64;
    let mut b = 0u64;
    let mut previous_end = 0usize;

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

        for &byte in &data[span.start..span.end] {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                let low_bit = crc & 1;
                crc >>= 1;
                if low_bit != 0 {
                    crc ^= 0xedb8_8320;
                }
            }

            a = (a + u64::from(byte)) % 65_521;
            b = (b + a) % 65_521;
        }

        previous_end = span.end;
    }

    Ok(Hashes {
        crc32: crc ^ 0xffff_ffff,
        adler32: ((b as u32) << 16) | (a as u32),
    })
}