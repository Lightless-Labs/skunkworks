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

const ADLER_MOD: u32 = 65_521;

fn crc32_update(mut crc: u32, data: &[u8]) -> u32 {
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
    crc
}

pub fn crc32(data: &[u8]) -> u32 {
    crc32_update(0xffff_ffff, data) ^ 0xffff_ffff
}

fn adler32_update(a: &mut u32, b: &mut u32, data: &[u8]) {
    for &byte in data {
        *a = (*a + u32::from(byte)) % ADLER_MOD;
        *b = (*b + *a) % ADLER_MOD;
    }
}

pub fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1, 0);
    adler32_update(&mut a, &mut b, data);
    (b << 16) | a
}

pub fn checksums(data: &[u8]) -> Hashes {
    let mut crc = crc32_update(0xffff_ffff, data);
    let (mut a, mut b) = (1, 0);
    adler32_update(&mut a, &mut b, data);
    crc ^= 0xffff_ffff;

    Hashes {
        crc32: crc,
        adler32: (b << 16) | a,
    }
}

pub fn checksums_spans(data: &[u8], spans: &[Span]) -> Result<Hashes, SpanError> {
    let mut crc = 0xffff_ffff;
    let (mut a, mut b) = (1, 0);
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

        let selected = &data[span.start..span.end];
        crc = crc32_update(crc, selected);
        adler32_update(&mut a, &mut b, selected);
        previous_end = span.end;
    }

    Ok(Hashes {
        crc32: crc ^ 0xffff_ffff,
        adler32: (b << 16) | a,
    })
}