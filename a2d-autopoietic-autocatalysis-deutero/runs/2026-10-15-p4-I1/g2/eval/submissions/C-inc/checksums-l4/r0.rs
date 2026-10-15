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

const ADLER_MOD: u32 = 65521;

fn crc32_update(mut crc: u32, data: &[u8]) -> u32 {
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            let low = crc & 1;
            crc >>= 1;
            if low != 0 {
                crc ^= 0xedb88320;
            }
        }
    }
    crc
}

pub fn crc32(data: &[u8]) -> u32 {
    crc32_update(0xffff_ffff, data) ^ 0xffff_ffff
}

fn adler_update(mut a: u32, mut b: u32, data: &[u8]) -> (u32, u32) {
    for &byte in data {
        a = (a + byte as u32) % ADLER_MOD;
        b = (b + a) % ADLER_MOD;
    }
    (a, b)
}

pub fn adler32(data: &[u8]) -> u32 {
    let (a, b) = adler_update(1, 0, data);
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

    pub fn checksum(&self) -> Hashes {
        Hashes {
            crc32: self.crc ^ 0xffff_ffff,
            adler32: (self.adler_b << 16) | self.adler_a,
        }
    }

    pub fn from_seed(crc32: u32, adler32: u32) -> Result<Self, SnapshotError> {
        let a = adler32 & 0xffff;
        let b = adler32 >> 16;
        if a >= ADLER_MOD {
            return Err(SnapshotError::Adler { component: 0 });
        }
        if b >= ADLER_MOD {
            return Err(SnapshotError::Adler { component: 1 });
        }
        Ok(Self {
            crc: crc32 ^ 0xffff_ffff,
            adler_a: a,
            adler_b: b,
        })
    }

    pub fn update(&mut self, data: &[u8]) {
        self.crc = crc32_update(self.crc, data);
        (self.adler_a, self.adler_b) =
            adler_update(self.adler_a, self.adler_b, data);
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
        let checksums = self.checksum();
        format!(
            "c{:08x}:a{:08x}",
            checksums.crc32, checksums.adler32
        )
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

        let mut crc = 0u32;
        for offset in 1..9 {
            let digit = hex_digit(bytes[offset])
                .ok_or(SnapshotError::Hex { offset })?;
            crc = (crc << 4) | digit as u32;
        }

        let mut adler = 0u32;
        for offset in 11..19 {
            let digit = hex_digit(bytes[offset])
                .ok_or(SnapshotError::Hex { offset })?;
            adler = (adler << 4) | digit as u32;
        }

        Self::from_seed(crc, adler)
    }
}

impl Default for Stream {
    fn default() -> Self {
        Self::new()
    }
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
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
    let mut a = 0u32;
    let mut b = 0u32;
    for &byte in data {
        a = (a + byte as u32) % 255;
        b = (b + a) % 255;
    }
    ((b as u16) << 8) | a as u16
}

fn fletcher32_update(mut a: u32, mut b: u32, data: &[u8]) -> (u32, u32, Option<u8>) {
    let mut pending = None;
    for &byte in data {
        if let Some(high) = pending.take() {
            let word = ((high as u32) << 8) | byte as u32;
            a = (a + word) % 65535;
            b = (b + a) % 65535;
        } else {
            pending = Some(byte);
        }
    }
    (a, b, pending)
}

fn fletcher32_finish(a: u32, b: u32, pending: Option<u8>) -> u32 {
    let (a, b) = if let Some(high) = pending {
        let a = (a + ((high as u32) << 8)) % 65535;
        let b = (b + a) % 65535;
        (a, b)
    } else {
        (a, b)
    };
    (b << 16) | a
}

pub fn fletcher32(data: &[u8]) -> u32 {
    let (a, b, pending) = fletcher32_update(0, 0, data);
    fletcher32_finish(a, b, pending)
}

pub fn fletcher32_spans(data: &[u8], spans: &[Span]) -> Result<u32, SpanError> {
    validate_spans(data.len(), spans)?;
    let mut a = 0u32;
    let mut b = 0u32;
    let mut pending = None;
    for span in spans {
        (a, b, pending) =
            fletcher32_update(a, b, &data[span.start..span.end]);
    }
    Ok(fletcher32_finish(a, b, pending))
}

pub fn crc16(data: &[u8], variant: Crc16) -> u16 {
    match variant {
        Crc16::Xmodem | Crc16::False => {
            let mut crc = if variant == Crc16::False {
                0xffffu16
            } else {
                0
            };
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

pub fn multi_checksums(
    data: &[u8],
    spans: &[Span],
    variant: Crc16,
) -> Result<Multi, SpanError> {
    validate_spans(data.len(), spans)?;

    let mut stream = Stream::new();
    let mut f16_a = 0u32;
    let mut f16_b = 0u32;
    let mut f32_a = 0u32;
    let mut f32_b = 0u32;
    let mut f32_pending = None;
    let mut c16 = match variant {
        Crc16::Xmodem => 0,
        Crc16::False | Crc16::X25 => 0xffff,
    };

    for span in spans {
        let selected = &data[span.start..span.end];
        stream.update(selected);

        for &byte in selected {
            f16_a = (f16_a + byte as u32) % 255;
            f16_b = (f16_b + f16_a) % 255;
        }

        (f32_a, f32_b, f32_pending) =
            fletcher32_update(f32_a, f32_b, selected);

        match variant {
            Crc16::Xmodem | Crc16::False => {
                for &byte in selected {
                    c16 ^= (byte as u16) << 8;
                    for _ in 0..8 {
                        let top = c16 & 0x8000;
                        c16 = c16.wrapping_shl(1);
                        if top != 0 {
                            c16 ^= 0x1021;
                        }
                    }
                }
            }
            Crc16::X25 => {
                for &byte in selected {
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

    if variant == Crc16::X25 {
        c16 ^= 0xffff;
    }

    Ok(Multi {
        base: stream.checksum(),
        fletcher16: ((f16_b as u16) << 8) | f16_a as u16,
        fletcher32: fletcher32_finish(f32_a, f32_b, f32_pending),
        crc16: c16,
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
    for index in 0..32 {
        square[index] = gf2_matrix_times(matrix, matrix[index]);
    }
}

pub fn crc32_combine(left: u32, right: u32, right_len: u64) -> u32 {
    if right_len == 0 {
        return left;
    }

    let mut odd = [0u32; 32];
    let mut even = [0u32; 32];

    odd[0] = 0xedb88320;
    let mut row = 1u32;
    for slot in odd.iter_mut().skip(1) {
        *slot = row;
        row <<= 1;
    }

    gf2_matrix_square(&mut even, &odd);
    gf2_matrix_square(&mut odd, &even);

    let mut len = right_len;
    let mut crc = left;

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

pub fn combine_parts(parts: &[Part]) -> Result<Part, CombineError> {
    let mut total_len = 0u64;
    for (index, part) in parts.iter().enumerate() {
        total_len = total_len
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
        len: total_len,
    })
}

pub fn manifest(data: &[u8], spans: &[Span]) -> Result<String, SpanError> {
    validate_spans(data.len(), spans)?;
    let mut stream = Stream::new();
    let mut len = 0u64;
    for span in spans {
        let selected = &data[span.start..span.end];
        stream.update(selected);
        len += selected.len() as u64;
    }
    let result = stream.checksum();
    Ok(format!(
        "len={:016x};crc={:08x};adler={:08x}",
        len, result.crc32, result.adler32
    ))
}

fn parse_hex(bytes: &[u8], start: usize, end: usize) -> Result<u64, VerifyError> {
    let mut value = 0u64;
    for offset in start..end {
        let digit = hex_digit(bytes[offset])
            .ok_or(VerifyError::Hex { offset })?;
        value = (value << 4) | digit as u64;
    }
    Ok(value)
}

pub fn verify(
    data: &[u8],
    spans: &[Span],
    token: &str,
) -> Result<(), VerifyError> {
    validate_spans(data.len(), spans).map_err(VerifyError::Span)?;

    let bytes = token.as_bytes();
    if bytes.len() != 48
        || &bytes[0..4] != b"len="
        || &bytes[20..25] != b";crc="
        || &bytes[33..40] != b";adler="
    {
        return Err(VerifyError::Shape);
    }

    let parsed_len = parse_hex(bytes, 4, 20)?;
    let parsed_crc = parse_hex(bytes, 25, 33)? as u32;
    let parsed_adler = parse_hex(bytes, 40, 48)? as u32;

    let mut stream = Stream::new();
    let mut actual_len = 0u64;
    for span in spans {
        let selected = &data[span.start..span.end];
        actual_len += selected.len() as u64;
        stream.update(selected);
    }

    if parsed_len != actual_len {
        return Err(VerifyError::Length);
    }

    let actual = stream.checksum();
    if parsed_crc != actual.crc32 {
        return Err(VerifyError::Mismatch {
            field: 0,
            expected: parsed_crc,
            actual: actual.crc32,
        });
    }
    if parsed_adler != actual.adler32 {
        return Err(VerifyError::Mismatch {
            field: 1,
            expected: parsed_adler,
            actual: actual.adler32,
        });
    }
    Ok(())
}

fn main() {}