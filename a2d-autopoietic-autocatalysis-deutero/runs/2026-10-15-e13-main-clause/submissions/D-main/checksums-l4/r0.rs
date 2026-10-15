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
        for &byte in data {
            self.crc = crc32_byte(self.crc, byte);
            self.adler_a = (self.adler_a + u32::from(byte)) % 65521;
            self.adler_b = (self.adler_b + self.adler_a) % 65521;
        }
    }

    pub fn checksum(&self) -> Hashes {
        Hashes {
            crc32: self.crc ^ 0xffff_ffff,
            adler32: (self.adler_b << 16) | self.adler_a,
        }
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

        for offset in (1..9).chain(11..19) {
            if hex_value(bytes[offset]).is_none() {
                return Err(SnapshotError::Hex { offset });
            }
        }

        let crc = parse_hex_u32(bytes, 1, 9);
        let adler = parse_hex_u32(bytes, 11, 19);
        Self::from_seed(crc, adler)
    }
}

impl Default for Stream {
    fn default() -> Self {
        Self::new()
    }
}

fn crc32_byte(mut crc: u32, byte: u8) -> u32 {
    crc ^= u32::from(byte);
    for _ in 0..8 {
        crc = (crc >> 1) ^ if crc & 1 != 0 { 0xedb8_8320 } else { 0 };
    }
    crc
}

pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffff;
    for &byte in data {
        crc = crc32_byte(crc, byte);
    }
    crc ^ 0xffff_ffff
}

pub fn adler32(data: &[u8]) -> u32 {
    let mut a = 1_u32;
    let mut b = 0_u32;
    for &byte in data {
        a = (a + u32::from(byte)) % 65521;
        b = (b + a) % 65521;
    }
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
        if index != 0 && span.start < previous_end {
            return Err(SpanError::Overlap { span: index });
        }
        previous_end = span.end;
    }
    Ok(())
}

pub fn checksums_spans(data: &[u8], spans: &[Span]) -> Result<Hashes, SpanError> {
    validate_spans(data.len(), spans)?;
    let mut stream = Stream::new();
    for span in spans {
        stream.update(&data[span.start..span.end]);
    }
    Ok(stream.checksum())
}

pub fn fletcher16(data: &[u8]) -> u16 {
    let mut a = 0_u32;
    let mut b = 0_u32;
    for &byte in data {
        a = (a + u32::from(byte)) % 255;
        b = (b + a) % 255;
    }
    ((b << 8) | a) as u16
}

fn fletcher32_word(a: &mut u32, b: &mut u32, word: u16) {
    *a = (*a + u32::from(word)) % 65535;
    *b = (*b + *a) % 65535;
}

pub fn fletcher32(data: &[u8]) -> u32 {
    let mut a = 0_u32;
    let mut b = 0_u32;
    let mut chunks = data.chunks_exact(2);
    for pair in &mut chunks {
        fletcher32_word(&mut a, &mut b, u16::from_be_bytes([pair[0], pair[1]]));
    }
    if let Some(&byte) = chunks.remainder().first() {
        fletcher32_word(&mut a, &mut b, u16::from(byte) << 8);
    }
    (b << 16) | a
}

pub fn fletcher32_spans(data: &[u8], spans: &[Span]) -> Result<u32, SpanError> {
    validate_spans(data.len(), spans)?;
    let mut a = 0_u32;
    let mut b = 0_u32;
    let mut pending = None;

    for span in spans {
        for &byte in &data[span.start..span.end] {
            if let Some(high) = pending.take() {
                fletcher32_word(&mut a, &mut b, u16::from_be_bytes([high, byte]));
            } else {
                pending = Some(byte);
            }
        }
    }

    if let Some(high) = pending {
        fletcher32_word(&mut a, &mut b, u16::from(high) << 8);
    }
    Ok((b << 16) | a)
}

pub fn crc16(data: &[u8], variant: Crc16) -> u16 {
    let mut crc = match variant {
        Crc16::Xmodem => 0_u16,
        Crc16::False | Crc16::X25 => 0xffff,
    };

    match variant {
        Crc16::Xmodem | Crc16::False => {
            for &byte in data {
                crc ^= u16::from(byte) << 8;
                for _ in 0..8 {
                    crc = (crc << 1) ^ if crc & 0x8000 != 0 { 0x1021 } else { 0 };
                }
            }
            crc
        }
        Crc16::X25 => {
            for &byte in data {
                crc ^= u16::from(byte);
                for _ in 0..8 {
                    crc = (crc >> 1) ^ if crc & 1 != 0 { 0x8408 } else { 0 };
                }
            }
            crc ^ 0xffff
        }
    }
}

pub fn multi_checksums(
    data: &[u8],
    spans: &[Span],
    variant: Crc16,
) -> Result<Multi, SpanError> {
    validate_spans(data.len(), spans)?;

    let mut stream = Stream::new();
    let mut f16_a = 0_u32;
    let mut f16_b = 0_u32;
    let mut f32_a = 0_u32;
    let mut f32_b = 0_u32;
    let mut f32_pending = None;
    let mut c16 = match variant {
        Crc16::Xmodem => 0_u16,
        Crc16::False | Crc16::X25 => 0xffff,
    };

    for span in spans {
        for &byte in &data[span.start..span.end] {
            stream.update(&[byte]);

            f16_a = (f16_a + u32::from(byte)) % 255;
            f16_b = (f16_b + f16_a) % 255;

            if let Some(high) = f32_pending.take() {
                fletcher32_word(
                    &mut f32_a,
                    &mut f32_b,
                    u16::from_be_bytes([high, byte]),
                );
            } else {
                f32_pending = Some(byte);
            }

            match variant {
                Crc16::Xmodem | Crc16::False => {
                    c16 ^= u16::from(byte) << 8;
                    for _ in 0..8 {
                        c16 = (c16 << 1)
                            ^ if c16 & 0x8000 != 0 { 0x1021 } else { 0 };
                    }
                }
                Crc16::X25 => {
                    c16 ^= u16::from(byte);
                    for _ in 0..8 {
                        c16 = (c16 >> 1)
                            ^ if c16 & 1 != 0 { 0x8408 } else { 0 };
                    }
                }
            }
        }
    }

    if let Some(high) = f32_pending {
        fletcher32_word(&mut f32_a, &mut f32_b, u16::from(high) << 8);
    }

    if variant == Crc16::X25 {
        c16 ^= 0xffff;
    }

    Ok(Multi {
        base: stream.checksum(),
        fletcher16: ((f16_b << 8) | f16_a) as u16,
        fletcher32: (f32_b << 16) | f32_a,
        crc16: c16,
    })
}

fn matrix_times(matrix: &[u32; 32], mut vector: u32) -> u32 {
    let mut sum = 0_u32;
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

fn matrix_square(square: &mut [u32; 32], matrix: &[u32; 32]) {
    for index in 0..32 {
        square[index] = matrix_times(matrix, matrix[index]);
    }
}

pub fn crc32_combine(left: u32, right: u32, right_len: u64) -> u32 {
    if right_len == 0 {
        return left;
    }

    let mut odd = [0_u32; 32];
    odd[0] = 0xedb8_8320;
    let mut row = 1_u32;
    for slot in odd.iter_mut().skip(1) {
        *slot = row;
        row <<= 1;
    }

    let mut even = [0_u32; 32];
    matrix_square(&mut even, &odd);
    matrix_square(&mut odd, &even);

    let mut crc = left;
    let mut len = right_len;
    loop {
        matrix_square(&mut even, &odd);
        if len & 1 != 0 {
            crc = matrix_times(&even, crc);
        }
        len >>= 1;
        if len == 0 {
            break;
        }

        matrix_square(&mut odd, &even);
        if len & 1 != 0 {
            crc = matrix_times(&odd, crc);
        }
        len >>= 1;
        if len == 0 {
            break;
        }
    }
    crc ^ right
}

pub fn combine_parts(parts: &[Part]) -> Result<Part, CombineError> {
    let mut total = 0_u64;
    for (index, part) in parts.iter().enumerate() {
        total = total
            .checked_add(part.len)
            .ok_or(CombineError::LengthOverflow { part: index })?;
    }

    for (index, part) in parts.iter().enumerate() {
        if part.len == 0 && part.crc32 != 0 {
            return Err(CombineError::EmptyCrc { part: index });
        }
    }

    let mut crc = 0_u32;
    for part in parts {
        crc = crc32_combine(crc, part.crc32, part.len);
    }
    Ok(Part { crc32: crc, len: total })
}

fn selected_stats(data: &[u8], spans: &[Span]) -> (u64, Hashes) {
    let mut len = 0_u64;
    let mut stream = Stream::new();
    for span in spans {
        let selected = &data[span.start..span.end];
        len += selected.len() as u64;
        stream.update(selected);
    }
    (len, stream.checksum())
}

pub fn manifest(data: &[u8], spans: &[Span]) -> Result<String, SpanError> {
    validate_spans(data.len(), spans)?;
    let (len, hashes) = selected_stats(data, spans);
    Ok(format!(
        "len={len:016x};crc={:08x};adler={:08x}",
        hashes.crc32, hashes.adler32
    ))
}

fn hex_value(byte: u8) -> Option<u32> {
    match byte {
        b'0'..=b'9' => Some(u32::from(byte - b'0')),
        b'a'..=b'f' => Some(u32::from(byte - b'a' + 10)),
        _ => None,
    }
}

fn parse_hex_u32(bytes: &[u8], start: usize, end: usize) -> u32 {
    let mut value = 0_u32;
    for &byte in &bytes[start..end] {
        value = (value << 4) | hex_value(byte).unwrap();
    }
    value
}

fn parse_hex_u64(bytes: &[u8], start: usize, end: usize) -> u64 {
    let mut value = 0_u64;
    for &byte in &bytes[start..end] {
        value = (value << 4) | u64::from(hex_value(byte).unwrap());
    }
    value
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

    for offset in (4..20).chain(25..33).chain(40..48) {
        if hex_value(bytes[offset]).is_none() {
            return Err(VerifyError::Hex { offset });
        }
    }

    let expected_len = parse_hex_u64(bytes, 4, 20);
    let expected_crc = parse_hex_u32(bytes, 25, 33);
    let expected_adler = parse_hex_u32(bytes, 40, 48);
    let (actual_len, actual) = selected_stats(data, spans);

    if expected_len != actual_len {
        return Err(VerifyError::Length);
    }
    if expected_crc != actual.crc32 {
        return Err(VerifyError::Mismatch {
            field: 0,
            expected: expected_crc,
            actual: actual.crc32,
        });
    }
    if expected_adler != actual.adler32 {
        return Err(VerifyError::Mismatch {
            field: 1,
            expected: expected_adler,
            actual: actual.adler32,
        });
    }
    Ok(())
}

fn main() {}