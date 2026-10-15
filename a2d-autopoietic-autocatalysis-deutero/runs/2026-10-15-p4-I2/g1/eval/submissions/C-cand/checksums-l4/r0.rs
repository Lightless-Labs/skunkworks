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

#[derive(Debug, Clone)]
pub struct Stream {
    crc: u32,
    a: u32,
    b: u32,
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
    Mismatch { field: usize, expected: u32, actual: u32 },
}

fn crc32_byte(mut crc: u32, byte: u8) -> u32 {
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
        crc = crc32_byte(crc, byte);
    }
    crc ^ 0xffff_ffff
}

fn adler_byte(a: u32, b: u32, byte: u8) -> (u32, u32) {
    let a = (a + byte as u32) % 65521;
    let b = (b + a) % 65521;
    (a, b)
}

pub fn adler32(data: &[u8]) -> u32 {
    let mut a = 1;
    let mut b = 0;
    for &byte in data {
        (a, b) = adler_byte(a, b, byte);
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
struct Accumulator {
    crc: u32,
    a: u32,
    b: u32,
    f16a: u32,
    f16b: u32,
    f32a: u32,
    f32b: u32,
    f32_pending: Option<u8>,
    crc16: u16,
}

impl Accumulator {
    fn new(variant: Crc16) -> Self {
        Self {
            crc: 0xffff_ffff,
            a: 1,
            b: 0,
            f16a: 0,
            f16b: 0,
            f32a: 0,
            f32b: 0,
            f32_pending: None,
            crc16: match variant {
                Crc16::Xmodem => 0,
                Crc16::False | Crc16::X25 => 0xffff,
            },
        }
    }

    fn update(&mut self, byte: u8, variant: Crc16) {
        self.crc = crc32_byte(self.crc, byte);
        (self.a, self.b) = adler_byte(self.a, self.b, byte);

        self.f16a = (self.f16a + byte as u32) % 255;
        self.f16b = (self.f16b + self.f16a) % 255;

        if let Some(high) = self.f32_pending.take() {
            self.fletcher32_word(((high as u16) << 8) | byte as u16);
        } else {
            self.f32_pending = Some(byte);
        }

        match variant {
            Crc16::Xmodem | Crc16::False => {
                self.crc16 ^= (byte as u16) << 8;
                for _ in 0..8 {
                    let top = self.crc16 & 0x8000;
                    self.crc16 = self.crc16.wrapping_shl(1);
                    if top != 0 {
                        self.crc16 ^= 0x1021;
                    }
                }
            }
            Crc16::X25 => {
                self.crc16 ^= byte as u16;
                for _ in 0..8 {
                    let low = self.crc16 & 1;
                    self.crc16 >>= 1;
                    if low != 0 {
                        self.crc16 ^= 0x8408;
                    }
                }
            }
        }
    }

    fn fletcher32_word(&mut self, word: u16) {
        self.f32a = (self.f32a + word as u32) % 65535;
        self.f32b = (self.f32b + self.f32a) % 65535;
    }

    fn finish_fletcher32(&mut self) {
        if let Some(high) = self.f32_pending.take() {
            self.fletcher32_word((high as u16) << 8);
        }
    }

    fn hashes(&self) -> Hashes {
        Hashes {
            crc32: self.crc ^ 0xffff_ffff,
            adler32: (self.b << 16) | self.a,
        }
    }

    fn multi(&mut self, variant: Crc16) -> Multi {
        self.finish_fletcher32();
        Multi {
            base: self.hashes(),
            fletcher16: ((self.f16b as u16) << 8) | self.f16a as u16,
            fletcher32: (self.f32b << 16) | self.f32a,
            crc16: match variant {
                Crc16::X25 => self.crc16 ^ 0xffff,
                Crc16::Xmodem | Crc16::False => self.crc16,
            },
        }
    }
}

pub fn checksums_spans(data: &[u8], spans: &[Span]) -> Result<Hashes, SpanError> {
    validate_spans(data.len(), spans)?;
    let mut acc = Accumulator::new(Crc16::Xmodem);
    for span in spans {
        for &byte in &data[span.start..span.end] {
            acc.update(byte, Crc16::Xmodem);
        }
    }
    Ok(acc.hashes())
}

impl Stream {
    pub fn new() -> Self {
        Self {
            crc: 0xffff_ffff,
            a: 1,
            b: 0,
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

    pub fn checksum(&self) -> Hashes {
        Hashes {
            crc32: self.crc ^ 0xffff_ffff,
            adler32: (self.b << 16) | self.a,
        }
    }

    pub fn update(&mut self, data: &[u8]) {
        for &byte in data {
            self.crc = crc32_byte(self.crc, byte);
            (self.a, self.b) = adler_byte(self.a, self.b, byte);
        }
    }

    pub fn update_spans(&mut self, data: &[u8], spans: &[Span]) -> Result<(), SpanError> {
        validate_spans(data.len(), spans)?;
        let mut next = self.clone();
        for span in spans {
            next.update(&data[span.start..span.end]);
        }
        *self = next;
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
        let crc = parse_hex_u32(bytes, 1, 9)?;
        let adler = parse_hex_u32(bytes, 11, 19)?;
        Self::from_seed(crc, adler)
    }
}

fn hex_value(byte: u8) -> Option<u32> {
    match byte {
        b'0'..=b'9' => Some((byte - b'0') as u32),
        b'a'..=b'f' => Some((byte - b'a' + 10) as u32),
        _ => None,
    }
}

fn parse_hex_u32(bytes: &[u8], start: usize, end: usize) -> Result<u32, SnapshotError> {
    let mut value = 0u32;
    for offset in start..end {
        let digit = hex_value(bytes[offset]).ok_or(SnapshotError::Hex { offset })?;
        value = (value << 4) | digit;
    }
    Ok(value)
}

pub fn fletcher16(data: &[u8]) -> u16 {
    let mut a = 0u32;
    let mut b = 0u32;
    for &byte in data {
        a = (a + byte as u32) % 255;
        b = (b + a) % 255;
    }
    ((b as u16) << 8) | a as u16
}

pub fn fletcher32(data: &[u8]) -> u32 {
    let mut acc = Accumulator::new(Crc16::Xmodem);
    for &byte in data {
        acc.update(byte, Crc16::Xmodem);
    }
    acc.finish_fletcher32();
    (acc.f32b << 16) | acc.f32a
}

pub fn fletcher32_spans(data: &[u8], spans: &[Span]) -> Result<u32, SpanError> {
    validate_spans(data.len(), spans)?;
    let mut acc = Accumulator::new(Crc16::Xmodem);
    for span in spans {
        for &byte in &data[span.start..span.end] {
            acc.update(byte, Crc16::Xmodem);
        }
    }
    acc.finish_fletcher32();
    Ok((acc.f32b << 16) | acc.f32a)
}

pub fn crc16(data: &[u8], variant: Crc16) -> u16 {
    let mut acc = Accumulator::new(variant);
    for &byte in data {
        acc.update(byte, variant);
    }
    match variant {
        Crc16::X25 => acc.crc16 ^ 0xffff,
        Crc16::Xmodem | Crc16::False => acc.crc16,
    }
}

pub fn multi_checksums(
    data: &[u8],
    spans: &[Span],
    variant: Crc16,
) -> Result<Multi, SpanError> {
    validate_spans(data.len(), spans)?;
    let mut acc = Accumulator::new(variant);
    for span in spans {
        for &byte in &data[span.start..span.end] {
            acc.update(byte, variant);
        }
    }
    Ok(acc.multi(variant))
}

pub fn crc32_combine(left: u32, right: u32, right_len: u64) -> u32 {
    if right_len == 0 {
        return left;
    }

    fn matrix_times(matrix: &[u32; 32], mut vector: u32) -> u32 {
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

    fn matrix_square(square: &mut [u32; 32], matrix: &[u32; 32]) {
        for i in 0..32 {
            square[i] = matrix_times(matrix, matrix[i]);
        }
    }

    let mut odd = [0u32; 32];
    let mut even = [0u32; 32];
    odd[0] = 0xedb88320;
    let mut row = 1u32;
    for slot in odd.iter_mut().skip(1) {
        *slot = row;
        row <<= 1;
    }

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

    let mut crc = 0;
    for part in parts {
        crc = crc32_combine(crc, part.crc32, part.len);
    }
    Ok(Part { crc32: crc, len: total })
}

pub fn manifest(data: &[u8], spans: &[Span]) -> Result<String, SpanError> {
    validate_spans(data.len(), spans)?;
    let mut acc = Accumulator::new(Crc16::Xmodem);
    let mut len = 0u64;
    for span in spans {
        let selected = &data[span.start..span.end];
        len += selected.len() as u64;
        for &byte in selected {
            acc.update(byte, Crc16::Xmodem);
        }
    }
    let hashes = acc.hashes();
    Ok(format!(
        "len={:016x};crc={:08x};adler={:08x}",
        len, hashes.crc32, hashes.adler32
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

    let mut parsed = [0u64; 3];
    for (field, (start, end)) in [(4, 20), (25, 33), (40, 48)].into_iter().enumerate() {
        let mut value = 0u64;
        for offset in start..end {
            let digit = hex_value(bytes[offset]).ok_or(VerifyError::Hex { offset })?;
            value = (value << 4) | digit as u64;
        }
        parsed[field] = value;
    }

    let mut acc = Accumulator::new(Crc16::Xmodem);
    let mut len = 0u64;
    for span in spans {
        let selected = &data[span.start..span.end];
        len += selected.len() as u64;
        for &byte in selected {
            acc.update(byte, Crc16::Xmodem);
        }
    }

    if parsed[0] != len {
        return Err(VerifyError::Length);
    }
    let actual = acc.hashes();
    let expected_crc = parsed[1] as u32;
    if expected_crc != actual.crc32 {
        return Err(VerifyError::Mismatch {
            field: 0,
            expected: expected_crc,
            actual: actual.crc32,
        });
    }
    let expected_adler = parsed[2] as u32;
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