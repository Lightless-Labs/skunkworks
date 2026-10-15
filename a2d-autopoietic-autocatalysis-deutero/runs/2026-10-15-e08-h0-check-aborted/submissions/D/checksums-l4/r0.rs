use std::fmt::Write;

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

fn crc32_update(mut reg: u32, data: &[u8]) -> u32 {
    for &byte in data {
        reg ^= byte as u32;
        for _ in 0..8 {
            let low = reg & 1;
            reg >>= 1;
            if low != 0 {
                reg ^= 0xedb88320;
            }
        }
    }
    reg
}

pub fn crc32(data: &[u8]) -> u32 {
    crc32_update(0xffff_ffff, data) ^ 0xffff_ffff
}

fn adler_update(a: &mut u32, b: &mut u32, data: &[u8]) {
    for &byte in data {
        *a = (*a + byte as u32) % 65521;
        *b = (*b + *a) % 65521;
    }
}

pub fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1, 0);
    adler_update(&mut a, &mut b, data);
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
    let (mut crc, mut a, mut b) = (0xffff_ffff, 1, 0);
    for span in spans {
        crc = crc32_update(crc, &data[span.start..span.end]);
        adler_update(&mut a, &mut b, &data[span.start..span.end]);
    }
    Ok(Hashes {
        crc32: crc ^ 0xffff_ffff,
        adler32: (b << 16) | a,
    })
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
        self.crc = crc32_update(self.crc, data);
        adler_update(&mut self.a, &mut self.b, data);
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

        let crc = parse_hex_bytes(bytes, 1, 9)?;
        let adler = parse_hex_bytes(bytes, 11, 19)?;
        Self::from_seed(crc, adler)
    }
}

impl Default for Stream {
    fn default() -> Self {
        Self::new()
    }
}

fn hex_value(byte: u8) -> Option<u32> {
    match byte {
        b'0'..=b'9' => Some((byte - b'0') as u32),
        b'a'..=b'f' => Some((byte - b'a' + 10) as u32),
        _ => None,
    }
}

fn parse_hex_bytes(bytes: &[u8], start: usize, end: usize) -> Result<u32, SnapshotError> {
    let mut value = 0u32;
    for offset in start..end {
        let digit = hex_value(bytes[offset]).ok_or(SnapshotError::Hex { offset })?;
        value = (value << 4) | digit;
    }
    Ok(value)
}

pub fn fletcher16(data: &[u8]) -> u16 {
    let (mut a, mut b) = (0u32, 0u32);
    for &byte in data {
        a = (a + byte as u32) % 255;
        b = (b + a) % 255;
    }
    ((b as u16) << 8) | a as u16
}

fn fletcher32_update(a: &mut u32, b: &mut u32, pending: &mut Option<u8>, data: &[u8]) {
    let mut i = 0;
    if let Some(high) = pending.take() {
        if let Some(&low) = data.first() {
            let word = ((high as u32) << 8) | low as u32;
            *a = (*a + word) % 65535;
            *b = (*b + *a) % 65535;
            i = 1;
        } else {
            *pending = Some(high);
            return;
        }
    }
    while i + 1 < data.len() {
        let word = ((data[i] as u32) << 8) | data[i + 1] as u32;
        *a = (*a + word) % 65535;
        *b = (*b + *a) % 65535;
        i += 2;
    }
    if i < data.len() {
        *pending = Some(data[i]);
    }
}

pub fn fletcher32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (0, 0);
    let mut pending = None;
    fletcher32_update(&mut a, &mut b, &mut pending, data);
    if let Some(high) = pending {
        a = (a + ((high as u32) << 8)) % 65535;
        b = (b + a) % 65535;
    }
    (b << 16) | a
}

pub fn fletcher32_spans(data: &[u8], spans: &[Span]) -> Result<u32, SpanError> {
    validate_spans(data.len(), spans)?;
    let (mut a, mut b) = (0, 0);
    let mut pending = None;
    for span in spans {
        fletcher32_update(&mut a, &mut b, &mut pending, &data[span.start..span.end]);
    }
    if let Some(high) = pending {
        a = (a + ((high as u32) << 8)) % 65535;
        b = (b + a) % 65535;
    }
    Ok((b << 16) | a)
}

pub fn crc16(data: &[u8], variant: Crc16) -> u16 {
    match variant {
        Crc16::Xmodem | Crc16::False => {
            let mut reg: u16 = if variant == Crc16::False { 0xffff } else { 0 };
            for &byte in data {
                reg ^= (byte as u16) << 8;
                for _ in 0..8 {
                    let top = reg & 0x8000;
                    reg <<= 1;
                    if top != 0 {
                        reg ^= 0x1021;
                    }
                }
            }
            reg
        }
        Crc16::X25 => {
            let mut reg: u16 = 0xffff;
            for &byte in data {
                reg ^= byte as u16;
                for _ in 0..8 {
                    let low = reg & 1;
                    reg >>= 1;
                    if low != 0 {
                        reg ^= 0x8408;
                    }
                }
            }
            reg ^ 0xffff
        }
    }
}

pub fn multi_checksums(
    data: &[u8],
    spans: &[Span],
    variant: Crc16,
) -> Result<Multi, SpanError> {
    validate_spans(data.len(), spans)?;
    let (mut crc, mut ad_a, mut ad_b) = (0xffff_ffff, 1, 0);
    let (mut f16_a, mut f16_b) = (0u32, 0u32);
    let (mut f32_a, mut f32_b) = (0u32, 0u32);
    let mut pending = None;
    let mut c16 = match variant {
        Crc16::False | Crc16::X25 => 0xffff,
        Crc16::Xmodem => 0,
    };

    for span in spans {
        let bytes = &data[span.start..span.end];
        crc = crc32_update(crc, bytes);
        adler_update(&mut ad_a, &mut ad_b, bytes);
        for &byte in bytes {
            f16_a = (f16_a + byte as u32) % 255;
            f16_b = (f16_b + f16_a) % 255;
        }
        fletcher32_update(&mut f32_a, &mut f32_b, &mut pending, bytes);
        match variant {
            Crc16::Xmodem | Crc16::False => {
                for &byte in bytes {
                    c16 ^= (byte as u16) << 8;
                    for _ in 0..8 {
                        let top = c16 & 0x8000;
                        c16 <<= 1;
                        if top != 0 {
                            c16 ^= 0x1021;
                        }
                    }
                }
            }
            Crc16::X25 => {
                for &byte in bytes {
                    c16 ^= byte as u16;
                    for _ in 0..8 {
                        let low = c16 & 1;
                        c16 >>= 1;
                        if low != 0 {
                            c16 ^= 0x8408;
                        }
                    }
                }
            }
        }
    }
    if let Some(high) = pending {
        f32_a = (f32_a + ((high as u32) << 8)) % 65535;
        f32_b = (f32_b + f32_a) % 65535;
    }
    if variant == Crc16::X25 {
        c16 ^= 0xffff;
    }
    Ok(Multi {
        base: Hashes {
            crc32: crc ^ 0xffff_ffff,
            adler32: (ad_b << 16) | ad_a,
        },
        fletcher16: ((f16_b as u16) << 8) | f16_a as u16,
        fletcher32: (f32_b << 16) | f32_a,
        crc16: c16,
    })
}

fn zero_byte_crc(mut reg: u32) -> u32 {
    for _ in 0..8 {
        let low = reg & 1;
        reg >>= 1;
        if low != 0 {
            reg ^= 0xedb88320;
        }
    }
    reg
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
    for n in 0..32 {
        square[n] = gf2_matrix_times(matrix, matrix[n]);
    }
}

pub fn crc32_combine(left: u32, right: u32, right_len: u64) -> u32 {
    if right_len == 0 {
        return left;
    }
    let mut matrix = [0u32; 32];
    let mut square = [0u32; 32];
    for bit in 0..32 {
        matrix[bit] = zero_byte_crc(1u32 << bit);
    }
    let mut len = right_len;
    let mut result = left;
    while len != 0 {
        if len & 1 != 0 {
            result = gf2_matrix_times(&matrix, result);
        }
        len >>= 1;
        if len != 0 {
            gf2_matrix_square(&mut square, &matrix);
            std::mem::swap(&mut matrix, &mut square);
        }
    }
    result ^ right
}

pub fn combine_parts(parts: &[Part]) -> Result<Part, CombineError> {
    let mut total = 0u64;
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
    let mut crc = 0u32;
    for part in parts {
        crc = crc32_combine(crc, part.crc32, part.len);
    }
    Ok(Part { crc32: crc, len: total })
}

pub fn manifest(data: &[u8], spans: &[Span]) -> Result<String, SpanError> {
    validate_spans(data.len(), spans)?;
    let mut len = 0u64;
    let (mut crc, mut a, mut b) = (0xffff_ffff, 1, 0);
    for span in spans {
        let bytes = &data[span.start..span.end];
        len += bytes.len() as u64;
        crc = crc32_update(crc, bytes);
        adler_update(&mut a, &mut b, bytes);
    }
    Ok(format!(
        "len={:016x};crc={:08x};adler={:08x}",
        len,
        crc ^ 0xffff_ffff,
        (b << 16) | a
    ))
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

    let len = parse_verify_hex(bytes, 4, 20)? as u64;
    let expected_crc = parse_verify_hex(bytes, 25, 33)?;
    let expected_adler = parse_verify_hex(bytes, 40, 48)?;

    let mut actual_len = 0u64;
    let (mut crc, mut a, mut b) = (0xffff_ffff, 1, 0);
    for span in spans {
        let selected = &data[span.start..span.end];
        actual_len += selected.len() as u64;
        crc = crc32_update(crc, selected);
        adler_update(&mut a, &mut b, selected);
    }
    if len != actual_len {
        return Err(VerifyError::Length);
    }
    let actual_crc = crc ^ 0xffff_ffff;
    if expected_crc != actual_crc {
        return Err(VerifyError::Mismatch {
            field: 0,
            expected: expected_crc,
            actual: actual_crc,
        });
    }
    let actual_adler = (b << 16) | a;
    if expected_adler != actual_adler {
        return Err(VerifyError::Mismatch {
            field: 1,
            expected: expected_adler,
            actual: actual_adler,
        });
    }
    Ok(())
}

fn parse_verify_hex(bytes: &[u8], start: usize, end: usize) -> Result<u32, VerifyError> {
    let mut value = 0u32;
    for offset in start..end {
        let digit = hex_value(bytes[offset]).ok_or(VerifyError::Hex { offset })?;
        value = (value << 4) | digit;
    }
    Ok(value)
}