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

    pub fn checksum(&self) -> Hashes {
        Hashes {
            crc32: self.crc ^ 0xffff_ffff,
            adler32: (self.adler_b << 16) | self.adler_a,
        }
    }

    pub fn update(&mut self, data: &[u8]) {
        update_crc(&mut self.crc, data);
        update_adler(&mut self.adler_a, &mut self.adler_b, data);
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

        let crc = parse_hex(&bytes[1..9]);
        let adler = parse_hex(&bytes[11..19]);
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

fn parse_hex(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0, |value, &byte| {
        (value << 4) | hex_value(byte).expect("validated hexadecimal digit")
    })
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

fn update_crc(crc: &mut u32, data: &[u8]) {
    for &byte in data {
        *crc ^= byte as u32;
        for _ in 0..8 {
            let low = *crc & 1;
            *crc >>= 1;
            if low != 0 {
                *crc ^= 0xedb8_8320;
            }
        }
    }
}

fn update_adler(a: &mut u32, b: &mut u32, data: &[u8]) {
    for &byte in data {
        *a = (*a + byte as u32) % 65521;
        *b = (*b + *a) % 65521;
    }
}

pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffff;
    update_crc(&mut crc, data);
    crc ^ 0xffff_ffff
}

pub fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1, 0);
    update_adler(&mut a, &mut b, data);
    (b << 16) | a
}

pub fn checksums(data: &[u8]) -> Hashes {
    Hashes {
        crc32: crc32(data),
        adler32: adler32(data),
    }
}

fn update_fletcher16(a: &mut u16, b: &mut u16, data: &[u8]) {
    for &byte in data {
        *a = ((*a as u32 + byte as u32) % 255) as u16;
        *b = ((*b as u32 + *a as u32) % 255) as u16;
    }
}

fn update_fletcher32_word(a: &mut u32, b: &mut u32, word: u16) {
    *a = (*a + word as u32) % 65535;
    *b = (*b + *a) % 65535;
}

pub fn fletcher16(data: &[u8]) -> u16 {
    let (mut a, mut b) = (0, 0);
    update_fletcher16(&mut a, &mut b, data);
    (b << 8) | a
}

pub fn fletcher32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (0, 0);
    let mut chunks = data.chunks_exact(2);
    for pair in &mut chunks {
        update_fletcher32_word(
            &mut a,
            &mut b,
            u16::from_be_bytes([pair[0], pair[1]]),
        );
    }
    if let Some(&last) = chunks.remainder().first() {
        update_fletcher32_word(&mut a, &mut b, (last as u16) << 8);
    }
    (b << 16) | a
}

pub fn fletcher32_spans(data: &[u8], spans: &[Span]) -> Result<u32, SpanError> {
    validate_spans(data.len(), spans)?;
    let (mut a, mut b) = (0, 0);
    let mut pending_high: Option<u8> = None;

    for span in spans {
        for &byte in &data[span.start..span.end] {
            if let Some(high) = pending_high.take() {
                update_fletcher32_word(
                    &mut a,
                    &mut b,
                    u16::from_be_bytes([high, byte]),
                );
            } else {
                pending_high = Some(byte);
            }
        }
    }

    if let Some(high) = pending_high {
        update_fletcher32_word(&mut a, &mut b, (high as u16) << 8);
    }

    Ok((b << 16) | a)
}

pub fn checksums_spans(data: &[u8], spans: &[Span]) -> Result<Hashes, SpanError> {
    validate_spans(data.len(), spans)?;
    let mut stream = Stream::new();
    for span in spans {
        stream.update(&data[span.start..span.end]);
    }
    Ok(stream.checksum())
}

pub fn crc16(data: &[u8], variant: Crc16) -> u16 {
    let mut crc = match variant {
        Crc16::Xmodem => 0,
        Crc16::False | Crc16::X25 => 0xffff,
    };

    match variant {
        Crc16::Xmodem | Crc16::False => {
            for &byte in data {
                crc ^= (byte as u16) << 8;
                for _ in 0..8 {
                    let top = crc & 0x8000;
                    crc <<= 1;
                    if top != 0 {
                        crc ^= 0x1021;
                    }
                }
            }
            crc
        }
        Crc16::X25 => {
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
    let (mut f16_a, mut f16_b) = (0, 0);
    let (mut f32_a, mut f32_b) = (0, 0);
    let mut f32_pending: Option<u8> = None;
    let mut crc16_state = match variant {
        Crc16::Xmodem => 0,
        Crc16::False | Crc16::X25 => 0xffff,
    };

    for span in spans {
        let selected = &data[span.start..span.end];
        stream.update(selected);
        update_fletcher16(&mut f16_a, &mut f16_b, selected);

        for &byte in selected {
            if let Some(high) = f32_pending.take() {
                update_fletcher32_word(
                    &mut f32_a,
                    &mut f32_b,
                    u16::from_be_bytes([high, byte]),
                );
            } else {
                f32_pending = Some(byte);
            }
        }

        match variant {
            Crc16::Xmodem | Crc16::False => {
                for &byte in selected {
                    crc16_state ^= (byte as u16) << 8;
                    for _ in 0..8 {
                        let top = crc16_state & 0x8000;
                        crc16_state <<= 1;
                        if top != 0 {
                            crc16_state ^= 0x1021;
                        }
                    }
                }
            }
            Crc16::X25 => {
                for &byte in selected {
                    crc16_state ^= byte as u16;
                    for _ in 0..8 {
                        let low = crc16_state & 1;
                        crc16_state >>= 1;
                        if low != 0 {
                            crc16_state ^= 0x8408;
                        }
                    }
                }
            }
        }
    }

    if let Some(high) = f32_pending {
        update_fletcher32_word(&mut f32_a, &mut f32_b, (high as u16) << 8);
    }

    let crc16_value = match variant {
        Crc16::X25 => crc16_state ^ 0xffff,
        Crc16::Xmodem | Crc16::False => crc16_state,
    };

    Ok(Multi {
        base: stream.checksum(),
        fletcher16: (f16_b << 8) | f16_a,
        fletcher32: (f32_b << 16) | f32_a,
        crc16: crc16_value,
    })
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
    for index in 0..32 {
        square[index] = gf2_matrix_times(matrix, matrix[index]);
    }
}

fn crc32_zero_byte_operator(value: u32) -> u32 {
    let mut result = value;
    for _ in 0..8 {
        let low = result & 1;
        result >>= 1;
        if low != 0 {
            result ^= 0xedb8_8320;
        }
    }
    result
}

pub fn crc32_combine(left: u32, right: u32, right_len: u64) -> u32 {
    if right_len == 0 {
        return left;
    }

    let mut matrix = [0u32; 32];
    let mut square = [0u32; 32];

    for (bit, item) in matrix.iter_mut().enumerate() {
        *item = crc32_zero_byte_operator(1u32 << bit);
    }

    let mut value = left;
    let mut length = right_len;
    while length != 0 {
        if length & 1 != 0 {
            value = gf2_matrix_times(&matrix, value);
        }
        length >>= 1;
        if length != 0 {
            gf2_matrix_square(&mut square, &matrix);
            matrix = square;
        }
    }

    value ^ right
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

    let mut combined = 0;
    for part in parts {
        combined = crc32_combine(combined, part.crc32, part.len);
    }

    Ok(Part {
        crc32: combined,
        len: total_len,
    })
}

fn selected_hashes_and_len(data: &[u8], spans: &[Span]) -> (Hashes, u64) {
    let mut stream = Stream::new();
    let mut len = 0u64;
    for span in spans {
        let selected = &data[span.start..span.end];
        stream.update(selected);
        len += selected.len() as u64;
    }
    (stream.checksum(), len)
}

pub fn manifest(data: &[u8], spans: &[Span]) -> Result<String, SpanError> {
    validate_spans(data.len(), spans)?;
    let (hashes, len) = selected_hashes_and_len(data, spans);
    Ok(format!(
        "len={len:016x};crc={:08x};adler={:08x}",
        hashes.crc32, hashes.adler32
    ))
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

    for offset in (4..20).chain(25..33).chain(40..48) {
        if hex_value(bytes[offset]).is_none() {
            return Err(VerifyError::Hex { offset });
        }
    }

    let parsed_len = parse_hex_u64(&bytes[4..20]);
    let expected_crc = parse_hex(&bytes[25..33]);
    let expected_adler = parse_hex(&bytes[40..48]);
    let (actual, actual_len) = selected_hashes_and_len(data, spans);

    if parsed_len != actual_len {
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

fn parse_hex_u64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0, |value, &byte| {
        (value << 4) | hex_value(byte).expect("validated hexadecimal digit") as u64
    })
}

fn main() {}