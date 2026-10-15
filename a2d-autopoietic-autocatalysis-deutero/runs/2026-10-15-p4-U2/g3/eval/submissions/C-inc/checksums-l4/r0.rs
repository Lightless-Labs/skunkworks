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

fn adler_update(mut a: u32, mut b: u32, data: &[u8]) -> (u32, u32) {
    for &byte in data {
        a = (a + byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    (a, b)
}

fn pack_adler(a: u32, b: u32) -> u32 {
    (b << 16) | a
}

pub fn adler32(data: &[u8]) -> u32 {
    let (a, b) = adler_update(1, 0, data);
    pack_adler(a, b)
}

pub fn checksums(data: &[u8]) -> Hashes {
    Hashes {
        crc32: crc32(data),
        adler32: adler32(data),
    }
}

fn validate_spans(data_len: usize, spans: &[Span]) -> Result<(), SpanError> {
    let mut previous_end = 0usize;
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
    let (mut a, mut b) = (1u32, 0u32);
    for span in spans {
        let selected = &data[span.start..span.end];
        crc = crc32_update(crc, selected);
        (a, b) = adler_update(a, b, selected);
    }
    Ok(Hashes {
        crc32: crc ^ 0xffff_ffff,
        adler32: pack_adler(a, b),
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
            adler32: pack_adler(self.a, self.b),
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
        (self.a, self.b) = adler_update(self.a, self.b, data);
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
        let crc = parse_hex(bytes, 1, 9)?;
        let adler = parse_hex(bytes, 11, 19)?;
        Self::from_seed(crc, adler)
    }
}

fn hex_value(byte: u8, offset: usize) -> Result<u32, SnapshotError> {
    match byte {
        b'0'..=b'9' => Ok((byte - b'0') as u32),
        b'a'..=b'f' => Ok((byte - b'a' + 10) as u32),
        _ => Err(SnapshotError::Hex { offset }),
    }
}

fn parse_hex(bytes: &[u8], start: usize, end: usize) -> Result<u32, SnapshotError> {
    let mut value = 0u32;
    for offset in start..end {
        value = (value << 4) | hex_value(bytes[offset], offset)?;
    }
    Ok(value)
}

fn fletcher16_update(mut a: u32, mut b: u32, data: &[u8]) -> (u32, u32) {
    for &byte in data {
        a = (a + byte as u32) % 255;
        b = (b + a) % 255;
    }
    (a, b)
}

pub fn fletcher16(data: &[u8]) -> u16 {
    let (a, b) = fletcher16_update(0, 0, data);
    ((b as u16) << 8) | a as u16
}

fn fletcher32_word(mut a: u32, mut b: u32, word: u16) -> (u32, u32) {
    a = (a + word as u32) % 65535;
    b = (b + a) % 65535;
    (a, b)
}

fn fletcher32_update(
    mut a: u32,
    mut b: u32,
    pending: &mut Option<u8>,
    data: &[u8],
) -> (u32, u32) {
    let mut index = 0;
    if let Some(high) = pending.take() {
        if !data.is_empty() {
            let word = ((high as u16) << 8) | data[0] as u16;
            (a, b) = fletcher32_word(a, b, word);
            index = 1;
        } else {
            *pending = Some(high);
            return (a, b);
        }
    }
    while index + 1 < data.len() {
        let word = ((data[index] as u16) << 8) | data[index + 1] as u16;
        (a, b) = fletcher32_word(a, b, word);
        index += 2;
    }
    if index < data.len() {
        *pending = Some(data[index]);
    }
    (a, b)
}

fn fletcher32_finish(a: u32, b: u32, pending: Option<u8>) -> u32 {
    let (a, b) = if let Some(high) = pending {
        fletcher32_word(a, b, (high as u16) << 8)
    } else {
        (a, b)
    };
    (b << 16) | a
}

pub fn fletcher32(data: &[u8]) -> u32 {
    let (a, b) = (0u32, 0u32);
    let mut pending = None;
    let (a, b) = fletcher32_update(a, b, &mut pending, data);
    fletcher32_finish(a, b, pending)
}

pub fn fletcher32_spans(data: &[u8], spans: &[Span]) -> Result<u32, SpanError> {
    validate_spans(data.len(), spans)?;
    let (mut a, mut b) = (0u32, 0u32);
    let mut pending = None;
    for span in spans {
        (a, b) = fletcher32_update(a, b, &mut pending, &data[span.start..span.end]);
    }
    Ok(fletcher32_finish(a, b, pending))
}

pub fn crc16(data: &[u8], variant: Crc16) -> u16 {
    let mut reg = match variant {
        Crc16::Xmodem => 0u16,
        Crc16::False | Crc16::X25 => 0xffff,
    };
    for &byte in data {
        match variant {
            Crc16::Xmodem | Crc16::False => {
                reg ^= (byte as u16) << 8;
                for _ in 0..8 {
                    let top = reg & 0x8000;
                    reg = reg.wrapping_shl(1);
                    if top != 0 {
                        reg ^= 0x1021;
                    }
                }
            }
            Crc16::X25 => {
                reg ^= byte as u16;
                for _ in 0..8 {
                    let low = reg & 1;
                    reg >>= 1;
                    if low != 0 {
                        reg ^= 0x8408;
                    }
                }
            }
        }
    }
    if variant == Crc16::X25 {
        reg ^ 0xffff
    } else {
        reg
    }
}

fn crc16_update(mut reg: u16, data: &[u8], variant: Crc16) -> u16 {
    for &byte in data {
        match variant {
            Crc16::Xmodem | Crc16::False => {
                reg ^= (byte as u16) << 8;
                for _ in 0..8 {
                    let top = reg & 0x8000;
                    reg = reg.wrapping_shl(1);
                    if top != 0 {
                        reg ^= 0x1021;
                    }
                }
            }
            Crc16::X25 => {
                reg ^= byte as u16;
                for _ in 0..8 {
                    let low = reg & 1;
                    reg >>= 1;
                    if low != 0 {
                        reg ^= 0x8408;
                    }
                }
            }
        }
    }
    reg
}

pub fn multi_checksums(
    data: &[u8],
    spans: &[Span],
    variant: Crc16,
) -> Result<Multi, SpanError> {
    validate_spans(data.len(), spans)?;
    let mut crc = 0xffff_ffff;
    let (mut adler_a, mut adler_b) = (1u32, 0u32);
    let (mut f16_a, mut f16_b) = (0u32, 0u32);
    let (mut f32_a, mut f32_b) = (0u32, 0u32);
    let mut f32_pending = None;
    let mut c16 = match variant {
        Crc16::Xmodem => 0,
        Crc16::False | Crc16::X25 => 0xffff,
    };

    for span in spans {
        let selected = &data[span.start..span.end];
        crc = crc32_update(crc, selected);
        (adler_a, adler_b) = adler_update(adler_a, adler_b, selected);
        (f16_a, f16_b) = fletcher16_update(f16_a, f16_b, selected);
        (f32_a, f32_b) =
            fletcher32_update(f32_a, f32_b, &mut f32_pending, selected);
        c16 = crc16_update(c16, selected, variant);
    }

    if variant == Crc16::X25 {
        c16 ^= 0xffff;
    }

    Ok(Multi {
        base: Hashes {
            crc32: crc ^ 0xffff_ffff,
            adler32: pack_adler(adler_a, adler_b),
        },
        fletcher16: ((f16_b as u16) << 8) | f16_a as u16,
        fletcher32: fletcher32_finish(f32_a, f32_b, f32_pending),
        crc16: c16,
    })
}

fn zero_byte_step(mut reg: u32) -> u32 {
    for _ in 0..8 {
        let low = reg & 1;
        reg >>= 1;
        if low != 0 {
            reg ^= 0xedb88320;
        }
    }
    reg
}

fn matrix_times(matrix: &[u32; 32], mut vector: u32) -> u32 {
    let mut sum = 0u32;
    let mut index = 0usize;
    while vector != 0 {
        if vector & 1 != 0 {
            sum ^= matrix[index];
        }
        vector >>= 1;
        index += 1;
    }
    sum
}

fn matrix_square(matrix: &[u32; 32]) -> [u32; 32] {
    let mut square = [0u32; 32];
    for index in 0..32 {
        square[index] = matrix_times(matrix, matrix[index]);
    }
    square
}

pub fn crc32_combine(left: u32, right: u32, right_len: u64) -> u32 {
    if right_len == 0 {
        return left;
    }
    let mut operator = [0u32; 32];
    for bit in 0..32 {
        operator[bit] = zero_byte_step(1u32 << bit);
    }
    let mut length = right_len;
    let mut value = left;
    while length != 0 {
        if length & 1 != 0 {
            value = matrix_times(&operator, value);
        }
        length >>= 1;
        if length != 0 {
            operator = matrix_square(&operator);
        }
    }
    value ^ right
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
    let mut combined = 0u32;
    for part in parts {
        combined = crc32_combine(combined, part.crc32, part.len);
    }
    Ok(Part {
        crc32: combined,
        len: total,
    })
}

fn to_hex(value: u64, width: usize) -> String {
    format!("{value:0width$x}")
}

pub fn manifest(data: &[u8], spans: &[Span]) -> Result<String, SpanError> {
    let hashes = checksums_spans(data, spans)?;
    let mut len = 0u64;
    for span in spans {
        len += (span.end - span.start) as u64;
    }
    Ok(format!(
        "len={:016x};crc={:08x};adler={:08x}",
        len, hashes.crc32, hashes.adler32
    ))
}

fn parse_hex_u64(bytes: &[u8], start: usize, end: usize) -> u64 {
    let mut value = 0u64;
    for &byte in &bytes[start..end] {
        let digit = match byte {
            b'0'..=b'9' => (byte - b'0') as u64,
            b'a'..=b'f' => (byte - b'a' + 10) as u64,
            _ => 0,
        };
        value = (value << 4) | digit;
    }
    value
}

fn parse_hex_u32(bytes: &[u8], start: usize, end: usize) -> u32 {
    let mut value = 0u32;
    for &byte in &bytes[start..end] {
        let digit = match byte {
            b'0'..=b'9' => (byte - b'0') as u32,
            b'a'..=b'f' => (byte - b'a' + 10) as u32,
            _ => 0,
        };
        value = (value << 4) | digit;
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

    for range in [4..20, 25..33, 40..48] {
        for offset in range {
            if !matches!(bytes[offset], b'0'..=b'9' | b'a'..=b'f') {
                return Err(VerifyError::Hex { offset });
            }
        }
    }

    let expected_len = parse_hex_u64(bytes, 4, 20);
    let expected_crc = parse_hex_u32(bytes, 25, 33);
    let expected_adler = parse_hex_u32(bytes, 40, 48);

    let mut selected_len = 0u64;
    for span in spans {
        selected_len += (span.end - span.start) as u64;
    }
    if expected_len != selected_len {
        return Err(VerifyError::Length);
    }

    let actual = checksums_spans(data, spans).map_err(VerifyError::Span)?;
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