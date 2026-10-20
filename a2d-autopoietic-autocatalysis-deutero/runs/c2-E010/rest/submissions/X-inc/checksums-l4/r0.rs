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
    crc_raw: u32,
    adler_a: u32,
    adler_b: u32,
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

const ADLER_MOD: u32 = 65521;

fn crc32_update_raw(mut crc: u32, data: &[u8]) -> u32 {
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
    crc32_update_raw(0xffff_ffff, data) ^ 0xffff_ffff
}

fn adler_update(a: &mut u32, b: &mut u32, data: &[u8]) {
    for &byte in data {
        *a = (*a + byte as u32) % ADLER_MOD;
        *b = (*b + *a) % ADLER_MOD;
    }
}

pub fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
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

fn for_each_selected<F: FnMut(&[u8])>(data: &[u8], spans: &[Span], mut f: F) {
    for span in spans {
        f(&data[span.start..span.end]);
    }
}

pub fn checksums_spans(data: &[u8], spans: &[Span]) -> Result<Hashes, SpanError> {
    validate_spans(data.len(), spans)?;
    let mut crc_raw = 0xffff_ffff;
    let (mut a, mut b) = (1u32, 0u32);
    for_each_selected(data, spans, |chunk| {
        crc_raw = crc32_update_raw(crc_raw, chunk);
        adler_update(&mut a, &mut b, chunk);
    });
    Ok(Hashes {
        crc32: crc_raw ^ 0xffff_ffff,
        adler32: (b << 16) | a,
    })
}

impl Stream {
    pub fn new() -> Self {
        Self {
            crc_raw: 0xffff_ffff,
            adler_a: 1,
            adler_b: 0,
        }
    }

    pub fn checksum(&self) -> Hashes {
        Hashes {
            crc32: self.crc_raw ^ 0xffff_ffff,
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
            crc_raw: crc32 ^ 0xffff_ffff,
            adler_a: a,
            adler_b: b,
        })
    }

    pub fn update(&mut self, data: &[u8]) {
        self.crc_raw = crc32_update_raw(self.crc_raw, data);
        adler_update(&mut self.adler_a, &mut self.adler_b, data);
    }

    pub fn update_spans(
        &mut self,
        data: &[u8],
        spans: &[Span],
    ) -> Result<(), SpanError> {
        validate_spans(data.len(), spans)?;
        for_each_selected(data, spans, |chunk| self.update(chunk));
        Ok(())
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }

    pub fn snapshot(&self) -> String {
        let sum = self.checksum();
        format!("c{:08x}:a{:08x}", sum.crc32, sum.adler32)
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
        let crc = parse_hex_u32(&bytes[1..9]);
        let adler = parse_hex_u32(&bytes[11..19]);
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

fn parse_hex_u32(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0u32, |value, &byte| {
        (value << 4) | hex_value(byte).unwrap()
    })
}

pub fn fletcher16(data: &[u8]) -> u16 {
    let (mut a, mut b) = (0u32, 0u32);
    for &byte in data {
        a = (a + byte as u32) % 255;
        b = (b + a) % 255;
    }
    ((b << 8) | a) as u16
}

#[derive(Clone)]
struct Fletcher32State {
    a: u32,
    b: u32,
    pending: Option<u8>,
}

impl Fletcher32State {
    fn new() -> Self {
        Self {
            a: 0,
            b: 0,
            pending: None,
        }
    }

    fn update(&mut self, data: &[u8]) {
        let mut index = 0usize;
        if let Some(high) = self.pending.take() {
            if !data.is_empty() {
                let word = ((high as u32) << 8) | data[0] as u32;
                self.add_word(word);
                index = 1;
            } else {
                self.pending = Some(high);
                return;
            }
        }
        while index + 1 < data.len() {
            let word = ((data[index] as u32) << 8) | data[index + 1] as u32;
            self.add_word(word);
            index += 2;
        }
        if index < data.len() {
            self.pending = Some(data[index]);
        }
    }

    fn add_word(&mut self, word: u32) {
        self.a = (self.a + word) % 65535;
        self.b = (self.b + self.a) % 65535;
    }

    fn finish(mut self) -> u32 {
        if let Some(high) = self.pending.take() {
            self.add_word((high as u32) << 8);
        }
        (self.b << 16) | self.a
    }
}

pub fn fletcher32(data: &[u8]) -> u32 {
    let mut state = Fletcher32State::new();
    state.update(data);
    state.finish()
}

pub fn fletcher32_spans(data: &[u8], spans: &[Span]) -> Result<u32, SpanError> {
    validate_spans(data.len(), spans)?;
    let mut state = Fletcher32State::new();
    for_each_selected(data, spans, |chunk| state.update(chunk));
    Ok(state.finish())
}

fn crc16_update(mut reg: u16, data: &[u8], variant: Crc16) -> u16 {
    match variant {
        Crc16::Xmodem | Crc16::False => {
            for &byte in data {
                reg ^= (byte as u16) << 8;
                for _ in 0..8 {
                    let top = reg & 0x8000;
                    reg = reg.wrapping_shl(1);
                    if top != 0 {
                        reg ^= 0x1021;
                    }
                }
            }
            reg
        }
        Crc16::X25 => {
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
            reg
        }
    }
}

pub fn crc16(data: &[u8], variant: Crc16) -> u16 {
    let initial = match variant {
        Crc16::Xmodem => 0,
        Crc16::False | Crc16::X25 => 0xffff,
    };
    let value = crc16_update(initial, data, variant);
    if variant == Crc16::X25 {
        value ^ 0xffff
    } else {
        value
    }
}

pub fn multi_checksums(
    data: &[u8],
    spans: &[Span],
    variant: Crc16,
) -> Result<Multi, SpanError> {
    validate_spans(data.len(), spans)?;
    let mut crc_raw = 0xffff_ffff;
    let (mut adler_a, mut adler_b) = (1u32, 0u32);
    let (mut f16_a, mut f16_b) = (0u32, 0u32);
    let mut f32 = Fletcher32State::new();
    let initial_crc16 = match variant {
        Crc16::Xmodem => 0,
        Crc16::False | Crc16::X25 => 0xffff,
    };
    let mut c16 = initial_crc16;

    for_each_selected(data, spans, |chunk| {
        crc_raw = crc32_update_raw(crc_raw, chunk);
        adler_update(&mut adler_a, &mut adler_b, chunk);
        for &byte in chunk {
            f16_a = (f16_a + byte as u32) % 255;
            f16_b = (f16_b + f16_a) % 255;
        }
        f32.update(chunk);
        c16 = crc16_update(c16, chunk, variant);
    });

    if variant == Crc16::X25 {
        c16 ^= 0xffff;
    }

    Ok(Multi {
        base: Hashes {
            crc32: crc_raw ^ 0xffff_ffff,
            adler32: (adler_b << 16) | adler_a,
        },
        fletcher16: ((f16_b << 8) | f16_a) as u16,
        fletcher32: f32.finish(),
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
    for n in 0..32 {
        square[n] = gf2_matrix_times(matrix, matrix[n]);
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
    for item in odd.iter_mut().skip(1) {
        *item = row;
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
    Ok(Part {
        crc32: crc,
        len: total,
    })
}

fn selected_len(spans: &[Span]) -> u64 {
    spans.iter().fold(0u64, |sum, span| {
        sum + (span.end - span.start) as u64
    })
}

pub fn manifest(data: &[u8], spans: &[Span]) -> Result<String, SpanError> {
    let sums = checksums_spans(data, spans)?;
    let len = selected_len(spans);
    Ok(format!(
        "len={:016x};crc={:08x};adler={:08x}",
        len, sums.crc32, sums.adler32
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
    for range in [4..20, 25..33, 40..48] {
        for offset in range {
            if hex_value(bytes[offset]).is_none() {
                return Err(VerifyError::Hex { offset });
            }
        }
    }

    let mut length = 0u64;
    for &byte in &bytes[4..20] {
        length = (length << 4) | hex_value(byte).unwrap() as u64;
    }
    let expected_crc = parse_hex_u32(&bytes[25..33]);
    let expected_adler = parse_hex_u32(&bytes[40..48]);
    let actual = checksums_spans(data, spans).map_err(VerifyError::Span)?;
    if length != selected_len(spans) {
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