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

fn validate_spans(data_len: usize, spans: &[Span]) -> Result<(), SpanError> {
    let mut previous_end = None;
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
        if let Some(end) = previous_end {
            if span.start < end {
                return Err(SpanError::Overlap { span: i });
            }
        }
        previous_end = Some(span.end);
    }
    Ok(())
}

fn crc32_step(mut crc: u32, byte: u8) -> u32 {
    crc ^= byte as u32;
    for _ in 0..8 {
        let low = crc & 1;
        crc >>= 1;
        if low != 0 {
            crc ^= 0xedb88320;
        }
    }
    crc
}

pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffff;
    for &byte in data {
        crc = crc32_step(crc, byte);
    }
    crc ^ 0xffff_ffff
}

fn adler_step(a: &mut u32, b: &mut u32, byte: u8) {
    const MOD: u32 = 65521;
    *a = (*a + byte as u32) % MOD;
    *b = (*b + *a) % MOD;
}

pub fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in data {
        adler_step(&mut a, &mut b, byte);
    }
    (b << 16) | a
}

pub fn checksums(data: &[u8]) -> Hashes {
    Hashes {
        crc32: crc32(data),
        adler32: adler32(data),
    }
}

fn checksums_validated<'a>(data: &'a [u8], spans: &[Span]) -> Hashes {
    let mut crc = 0xffff_ffff;
    let (mut a, mut b) = (1u32, 0u32);
    for span in spans {
        for &byte in &data[span.start..span.end] {
            crc = crc32_step(crc, byte);
            adler_step(&mut a, &mut b, byte);
        }
    }
    Hashes {
        crc32: crc ^ 0xffff_ffff,
        adler32: (b << 16) | a,
    }
}

pub fn checksums_spans(data: &[u8], spans: &[Span]) -> Result<Hashes, SpanError> {
    validate_spans(data.len(), spans)?;
    Ok(checksums_validated(data, spans))
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

impl Stream {
    pub fn new() -> Self {
        Self {
            crc: 0xffff_ffff,
            a: 1,
            b: 0,
        }
    }

    pub fn checksum(&self) -> Hashes {
        Hashes {
            crc32: self.crc ^ 0xffff_ffff,
            adler32: (self.b << 16) | self.a,
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
            a,
            b,
        })
    }

    pub fn update(&mut self, data: &[u8]) {
        for &byte in data {
            self.crc = crc32_step(self.crc, byte);
            adler_step(&mut self.a, &mut self.b, byte);
        }
    }

    pub fn update_spans(&mut self, data: &[u8], spans: &[Span]) -> Result<(), SpanError> {
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
        for (field, range) in [(0, 1..9), (1, 11..19)] {
            let mut value = 0u32;
            for offset in range {
                let digit = match bytes[offset] {
                    b'0'..=b'9' => bytes[offset] - b'0',
                    b'a'..=b'f' => bytes[offset] - b'a' + 10,
                    _ => return Err(SnapshotError::Hex { offset }),
                };
                value = (value << 4) | digit as u32;
            }
            values[field] = value;
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
    let (mut a, mut b) = (0u32, 0u32);
    for &byte in data {
        a = (a + byte as u32) % 255;
        b = (b + a) % 255;
    }
    ((b as u16) << 8) | a as u16
}

fn fletcher32_word(a: &mut u32, b: &mut u32, word: u16) {
    *a = (*a + word as u32) % 65535;
    *b = (*b + *a) % 65535;
}

pub fn fletcher32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (0u32, 0u32);
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
    let (mut a, mut b) = (0u32, 0u32);
    let mut high = None;
    for span in spans {
        for &byte in &data[span.start..span.end] {
            if let Some(first) = high.take() {
                fletcher32_word(&mut a, &mut b, ((first as u16) << 8) | byte as u16);
            } else {
                high = Some(byte);
            }
        }
    }
    if let Some(last) = high {
        fletcher32_word(&mut a, &mut b, (last as u16) << 8);
    }
    Ok((b << 16) | a)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Crc16 {
    Xmodem,
    False,
    X25,
}

pub fn crc16(data: &[u8], variant: Crc16) -> u16 {
    match variant {
        Crc16::Xmodem | Crc16::False => {
            let mut crc = if variant == Crc16::False { 0xffffu16 } else { 0 };
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
            let mut crc = 0xffffu16;
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
            crc ^ 0xffff
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Multi {
    pub base: Hashes,
    pub fletcher16: u16,
    pub fletcher32: u32,
    pub crc16: u16,
}

fn multi_validated(data: &[u8], spans: &[Span], variant: Crc16) -> Multi {
    let mut crc = 0xffff_ffff;
    let (mut ad_a, mut ad_b) = (1u32, 0u32);
    let (mut f16_a, mut f16_b) = (0u32, 0u32);
    let (mut f32_a, mut f32_b) = (0u32, 0u32);
    let mut pending = None;
    let mut crc16_value = match variant {
        Crc16::Xmodem => 0,
        Crc16::False | Crc16::X25 => 0xffff,
    };

    for span in spans {
        for &byte in &data[span.start..span.end] {
            crc = crc32_step(crc, byte);
            adler_step(&mut ad_a, &mut ad_b, byte);

            f16_a = (f16_a + byte as u32) % 255;
            f16_b = (f16_b + f16_a) % 255;

            if let Some(first) = pending.take() {
                fletcher32_word(
                    &mut f32_a,
                    &mut f32_b,
                    ((first as u16) << 8) | byte as u16,
                );
            } else {
                pending = Some(byte);
            }

            match variant {
                Crc16::Xmodem | Crc16::False => {
                    crc16_value ^= (byte as u16) << 8;
                    for _ in 0..8 {
                        let top = crc16_value & 0x8000;
                        crc16_value = crc16_value.wrapping_shl(1);
                        if top != 0 {
                            crc16_value ^= 0x1021;
                        }
                    }
                }
                Crc16::X25 => {
                    crc16_value ^= byte as u16;
                    for _ in 0..8 {
                        let low = crc16_value & 1;
                        crc16_value >>= 1;
                        if low != 0 {
                            crc16_value ^= 0x8408;
                        }
                    }
                }
            }
        }
    }

    if let Some(last) = pending {
        fletcher32_word(&mut f32_a, &mut f32_b, (last as u16) << 8);
    }

    if variant == Crc16::X25 {
        crc16_value ^= 0xffff;
    }

    Multi {
        base: Hashes {
            crc32: crc ^ 0xffff_ffff,
            adler32: (ad_b << 16) | ad_a,
        },
        fletcher16: ((f16_b as u16) << 8) | f16_a as u16,
        fletcher32: (f32_b << 16) | f32_a,
        crc16: crc16_value,
    }
}

pub fn multi_checksums(
    data: &[u8],
    spans: &[Span],
    variant: Crc16,
) -> Result<Multi, SpanError> {
    validate_spans(data.len(), spans)?;
    Ok(multi_validated(data, spans, variant))
}

fn gf2_matrix_times(matrix: &[u32; 32], mut vector: u32) -> u32 {
    let mut sum = 0;
    let mut index = 0;
    while vector != 0 {
        if vector & 1 != 0 {
            sum ^= matrix[index];
        }
        vector >>= 1;
        index += 1;
    }
    sum
}

fn gf2_matrix_square(square: &mut [u32; 32], matrix: &[u32; 32]) {
    for i in 0..32 {
        square[i] = gf2_matrix_times(matrix, matrix[i]);
    }
}

fn crc32_zero_byte_operator(value: u32) -> u32 {
    let mut result = value;
    for _ in 0..8 {
        let low = result & 1;
        result >>= 1;
        if low != 0 {
            result ^= 0xedb88320;
        }
    }
    result
}

pub fn crc32_combine(left: u32, right: u32, right_len: u64) -> u32 {
    if right_len == 0 {
        return left;
    }

    let mut operator = [0u32; 32];
    for i in 0..32 {
        operator[i] = crc32_zero_byte_operator(1u32 << i);
    }

    let mut value = left;
    let mut length = right_len;
    while length != 0 {
        if length & 1 != 0 {
            value = gf2_matrix_times(&operator, value);
        }
        length >>= 1;
        if length != 0 {
            let mut square = [0u32; 32];
            gf2_matrix_square(&mut square, &operator);
            operator = square;
        }
    }
    value ^ right
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Part {
    pub crc32: u32,
    pub len: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CombineError {
    LengthOverflow { part: usize },
    EmptyCrc { part: usize },
}

pub fn combine_parts(parts: &[Part]) -> Result<Part, CombineError> {
    let mut total = 0u64;
    for (i, part) in parts.iter().enumerate() {
        total = total
            .checked_add(part.len)
            .ok_or(CombineError::LengthOverflow { part: i })?;
    }
    for (i, part) in parts.iter().enumerate() {
        if part.len == 0 && part.crc32 != 0 {
            return Err(CombineError::EmptyCrc { part: i });
        }
    }

    let mut combined = 0u32;
    for part in parts {
        if part.len != 0 {
            combined = crc32_combine(combined, part.crc32, part.len);
        }
    }
    Ok(Part {
        crc32: combined,
        len: total,
    })
}

pub fn manifest(data: &[u8], spans: &[Span]) -> Result<String, SpanError> {
    validate_spans(data.len(), spans)?;
    let hashes = checksums_validated(data, spans);
    let len: u64 = spans
        .iter()
        .map(|span| (span.end - span.start) as u64)
        .sum();
    Ok(format!(
        "len={:016x};crc={:08x};adler={:08x}",
        len, hashes.crc32, hashes.adler32
    ))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifyError {
    Span(SpanError),
    Shape,
    Hex { offset: usize },
    Length,
    Mismatch {
        field: usize,
        expected: u32,
        actual: u32,
    },
}

fn parse_hex(bytes: &[u8], range: core::ops::Range<usize>) -> Result<u64, VerifyError> {
    let mut value = 0u64;
    for offset in range {
        let digit = match bytes[offset] {
            b'0'..=b'9' => bytes[offset] - b'0',
            b'a'..=b'f' => bytes[offset] - b'a' + 10,
            _ => return Err(VerifyError::Hex { offset }),
        };
        value = (value << 4) | digit as u64;
    }
    Ok(value)
}

pub fn verify(data: &[u8], spans: &[Span], token: &str) -> Result<(), VerifyError> {
    validate_spans(data.len(), spans).map_err(VerifyError::Span)?;

    let bytes = token.as_bytes();
    if bytes.len() != 48
        || &bytes[0..4] != b"len="
        || &bytes[20..25] != b";crc="
        || &bytes[33..40] != b";adler="
    {
        return Err(VerifyError::Shape);
    }

    let length = parse_hex(bytes, 4..20)?;
    let crc = parse_hex(bytes, 25..33)? as u32;
    let adler = parse_hex(bytes, 40..48)? as u32;

    let hashes = checksums_validated(data, spans);
    let selected_len: u64 = spans
        .iter()
        .map(|span| (span.end - span.start) as u64)
        .sum();
    if length != selected_len {
        return Err(VerifyError::Length);
    }
    if crc != hashes.crc32 {
        return Err(VerifyError::Mismatch {
            field: 0,
            expected: crc,
            actual: hashes.crc32,
        });
    }
    if adler != hashes.adler32 {
        return Err(VerifyError::Mismatch {
            field: 1,
            expected: adler,
            actual: hashes.adler32,
        });
    }
    Ok(())
}