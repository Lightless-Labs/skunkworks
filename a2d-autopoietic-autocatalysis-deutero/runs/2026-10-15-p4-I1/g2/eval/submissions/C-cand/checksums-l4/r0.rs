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
    let mut reg = 0xffff_ffffu32;
    for &byte in data {
        reg ^= byte as u32;
        for _ in 0..8 {
            reg = (reg >> 1) ^ if reg & 1 != 0 { 0xedb8_8320 } else { 0 };
        }
    }
    reg ^ 0xffff_ffff
}

pub fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in data {
        a = (a + byte as u32) % 65521;
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
    let mut previous_end = 0usize;
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
        if i > 0 && span.start < previous_end {
            return Err(SpanError::Overlap { span: i });
        }
        previous_end = span.end;
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct Accum {
    crc: u32,
    a: u32,
    b: u32,
    f16a: u32,
    f16b: u32,
    f32a: u32,
    f32b: u32,
    f32_pending: Option<u8>,
    c16: u16,
    c16_variant: Option<Crc16>,
}

impl Accum {
    fn new() -> Self {
        Self {
            crc: 0xffff_ffff,
            a: 1,
            b: 0,
            f16a: 0,
            f16b: 0,
            f32a: 0,
            f32b: 0,
            f32_pending: None,
            c16: 0,
            c16_variant: None,
        }
    }

    fn with_crc16(variant: Crc16) -> Self {
        let mut acc = Self::new();
        acc.c16_variant = Some(variant);
        acc.c16 = match variant {
            Crc16::Xmodem => 0,
            Crc16::False | Crc16::X25 => 0xffff,
        };
        acc
    }

    fn update(&mut self, data: &[u8]) {
        for &byte in data {
            self.crc ^= byte as u32;
            for _ in 0..8 {
                self.crc =
                    (self.crc >> 1) ^ if self.crc & 1 != 0 { 0xedb8_8320 } else { 0 };
            }

            self.a = (self.a + byte as u32) % 65521;
            self.b = (self.b + self.a) % 65521;

            self.f16a = (self.f16a + byte as u32) % 255;
            self.f16b = (self.f16b + self.f16a) % 255;

            if let Some(high) = self.f32_pending.take() {
                self.fletcher32_word(((high as u16) << 8) | byte as u16);
            } else {
                self.f32_pending = Some(byte);
            }

            if let Some(variant) = self.c16_variant {
                match variant {
                    Crc16::Xmodem | Crc16::False => {
                        self.c16 ^= (byte as u16) << 8;
                        for _ in 0..8 {
                            let top = self.c16 & 0x8000 != 0;
                            self.c16 = self.c16.wrapping_shl(1);
                            if top {
                                self.c16 ^= 0x1021;
                            }
                        }
                    }
                    Crc16::X25 => {
                        self.c16 ^= byte as u16;
                        for _ in 0..8 {
                            let low = self.c16 & 1 != 0;
                            self.c16 >>= 1;
                            if low {
                                self.c16 ^= 0x8408;
                            }
                        }
                    }
                }
            }
        }
    }

    fn fletcher32_word(&mut self, word: u16) {
        self.f32a = (self.f32a + word as u32) % 65535;
        self.f32b = (self.f32b + self.f32a) % 65535;
    }

    fn hashes(&self) -> Hashes {
        Hashes {
            crc32: self.crc ^ 0xffff_ffff,
            adler32: (self.b << 16) | self.a,
        }
    }

    fn fletcher32(&self) -> u32 {
        let (a, b) = if let Some(high) = self.f32_pending {
            let mut a = self.f32a;
            let mut b = self.f32b;
            a = (a + ((high as u16) << 8) as u32) % 65535;
            b = (b + a) % 65535;
            (a, b)
        } else {
            (self.f32a, self.f32b)
        };
        (b << 16) | a
    }

    fn crc16(&self) -> u16 {
        match self.c16_variant {
            Some(Crc16::X25) => self.c16 ^ 0xffff,
            _ => self.c16,
        }
    }
}

pub fn checksums_spans(data: &[u8], spans: &[Span]) -> Result<Hashes, SpanError> {
    validate_spans(data.len(), spans)?;
    let mut acc = Accum::new();
    for span in spans {
        acc.update(&data[span.start..span.end]);
    }
    Ok(acc.hashes())
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
            self.crc ^= byte as u32;
            for _ in 0..8 {
                self.crc =
                    (self.crc >> 1) ^ if self.crc & 1 != 0 { 0xedb8_8320 } else { 0 };
            }
            self.a = (self.a + byte as u32) % 65521;
            self.b = (self.b + self.a) % 65521;
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
        let crc = parse_hex(bytes, 1, 9)?;
        let adler = parse_hex(bytes, 11, 19)?;
        Self::from_seed(crc, adler)
    }
}

impl Default for Stream {
    fn default() -> Self {
        Self::new()
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
    for i in start..end {
        value = (value << 4) | hex_value(bytes[i], i)?;
    }
    Ok(value)
}

pub fn fletcher16(data: &[u8]) -> u16 {
    let (mut a, mut b) = (0u32, 0u32);
    for &byte in data {
        a = (a + byte as u32) % 255;
        b = (b + a) % 255;
    }
    ((b << 8) | a) as u16
}

pub fn fletcher32(data: &[u8]) -> u32 {
    let mut acc = Accum::new();
    acc.update(data);
    acc.fletcher32()
}

pub fn fletcher32_spans(data: &[u8], spans: &[Span]) -> Result<u32, SpanError> {
    validate_spans(data.len(), spans)?;
    let mut acc = Accum::new();
    for span in spans {
        acc.update(&data[span.start..span.end]);
    }
    Ok(acc.fletcher32())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Crc16 {
    Xmodem,
    False,
    X25,
}

pub fn crc16(data: &[u8], variant: Crc16) -> u16 {
    let mut acc = Accum::with_crc16(variant);
    acc.update(data);
    acc.crc16()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Multi {
    pub base: Hashes,
    pub fletcher16: u16,
    pub fletcher32: u32,
    pub crc16: u16,
}

pub fn multi_checksums(
    data: &[u8],
    spans: &[Span],
    variant: Crc16,
) -> Result<Multi, SpanError> {
    validate_spans(data.len(), spans)?;
    let mut acc = Accum::with_crc16(variant);
    for span in spans {
        acc.update(&data[span.start..span.end]);
    }
    Ok(Multi {
        base: acc.hashes(),
        fletcher16: ((acc.f16b << 8) | acc.f16a) as u16,
        fletcher32: acc.fletcher32(),
        crc16: acc.crc16(),
    })
}

fn gf2_matrix_times(matrix: &[u32; 32], mut vector: u32) -> u32 {
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

fn gf2_matrix_square(square: &mut [u32; 32], matrix: &[u32; 32]) {
    for i in 0..32 {
        square[i] = gf2_matrix_times(matrix, matrix[i]);
    }
}

pub fn crc32_combine(left: u32, right: u32, right_len: u64) -> u32 {
    if right_len == 0 {
        return left;
    }

    let mut odd = [0u32; 32];
    let mut even = [0u32; 32];

    odd[0] = 0xedb8_8320;
    let mut row = 1u32;
    for slot in odd.iter_mut().skip(1) {
        *slot = row;
        row <<= 1;
    }
    gf2_matrix_square(&mut even, &odd);
    gf2_matrix_square(&mut odd, &even);

    let mut crc = left;
    let mut len = right_len;
    loop {
        gf2_matrix_square(&mut even, &odd);
        if len & 1 != 0 {
            crc = gf2_matrix_times(&even, crc);
        }
        len >>= 1;
        if len == 0 {
            break;
        }

        gf2_matrix_square(&mut odd, &even);
        if len & 1 != 0 {
            crc = gf2_matrix_times(&odd, crc);
        }
        len >>= 1;
        if len == 0 {
            break;
        }
    }
    crc ^ right
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
        combined = crc32_combine(combined, part.crc32, part.len);
    }
    Ok(Part {
        crc32: combined,
        len: total,
    })
}

pub fn manifest(data: &[u8], spans: &[Span]) -> Result<String, SpanError> {
    validate_spans(data.len(), spans)?;
    let mut acc = Accum::new();
    let mut len = 0u64;
    for span in spans {
        let selected = &data[span.start..span.end];
        len += selected.len() as u64;
        acc.update(selected);
    }
    let hashes = acc.hashes();
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
        for i in range {
            match bytes[i] {
                b'0'..=b'9' | b'a'..=b'f' => {}
                _ => return Err(VerifyError::Hex { offset: i }),
            }
        }
    }

    let mut acc = Accum::new();
    let mut len = 0u64;
    for span in spans {
        let selected = &data[span.start..span.end];
        len += selected.len() as u64;
        acc.update(selected);
    }

    let expected_len = parse_hex_u64(bytes, 4, 20);
    if expected_len != len {
        return Err(VerifyError::Length);
    }
    let expected_crc = parse_hex(bytes, 25, 33).expect("digits were validated");
    let expected_adler = parse_hex(bytes, 40, 48).expect("digits were validated");
    let actual = acc.hashes();
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

fn parse_hex_u64(bytes: &[u8], start: usize, end: usize) -> u64 {
    let mut value = 0u64;
    for &byte in &bytes[start..end] {
        let digit = match byte {
            b'0'..=b'9' => (byte - b'0') as u64,
            _ => (byte - b'a' + 10) as u64,
        };
        value = (value << 4) | digit;
    }
    value
}

fn main() {}