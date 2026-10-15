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

impl Stream {
    pub fn new() -> Self {
        Self {
            crc_raw: 0xffff_ffff,
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
            crc_raw: crc32 ^ 0xffff_ffff,
            adler_a: a,
            adler_b: b,
        })
    }

    pub fn checksum(&self) -> Hashes {
        Hashes {
            crc32: self.crc_raw ^ 0xffff_ffff,
            adler32: (self.adler_b << 16) | self.adler_a,
        }
    }

    pub fn update(&mut self, data: &[u8]) {
        update_state(&mut self.crc_raw, &mut self.adler_a, &mut self.adler_b, data);
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

        let crc = parse_hex_u32(bytes, 1, 9).map_err(|offset| SnapshotError::Hex { offset })?;
        let adler =
            parse_hex_u32(bytes, 11, 19).map_err(|offset| SnapshotError::Hex { offset })?;
        Self::from_seed(crc, adler)
    }
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

pub fn crc32(data: &[u8]) -> u32 {
    let mut raw = 0xffff_ffff;
    for &byte in data {
        raw ^= byte as u32;
        for _ in 0..8 {
            let low = raw & 1;
            raw >>= 1;
            if low != 0 {
                raw ^= 0xedb8_8320;
            }
        }
    }
    raw ^ 0xffff_ffff
}

pub fn adler32(data: &[u8]) -> u32 {
    let mut a = 1u32;
    let mut b = 0u32;
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

pub fn fletcher32(data: &[u8]) -> u32 {
    let mut state = Fletcher32State::new();
    state.update(data);
    state.finish()
}

pub fn fletcher32_spans(data: &[u8], spans: &[Span]) -> Result<u32, SpanError> {
    validate_spans(data.len(), spans)?;
    let mut state = Fletcher32State::new();
    for span in spans {
        state.update(&data[span.start..span.end]);
    }
    Ok(state.finish())
}

pub fn crc16(data: &[u8], variant: Crc16) -> u16 {
    let mut state = Crc16State::new(variant);
    state.update(data);
    state.finish()
}

pub fn multi_checksums(
    data: &[u8],
    spans: &[Span],
    variant: Crc16,
) -> Result<Multi, SpanError> {
    validate_spans(data.len(), spans)?;

    let mut stream = Stream::new();
    let mut f16 = Fletcher16State::new();
    let mut f32 = Fletcher32State::new();
    let mut c16 = Crc16State::new(variant);

    for span in spans {
        let selected = &data[span.start..span.end];
        stream.update(selected);
        f16.update(selected);
        f32.update(selected);
        c16.update(selected);
    }

    Ok(Multi {
        base: stream.checksum(),
        fletcher16: f16.finish(),
        fletcher32: f32.finish(),
        crc16: c16.finish(),
    })
}

pub fn crc32_combine(left: u32, right: u32, right_len: u64) -> u32 {
    if right_len == 0 {
        return left;
    }

    let mut matrix = [0u32; 32];
    let mut square = [0u32; 32];

    matrix[0] = 0xedb8_8320;
    let mut row = 1u32;
    for entry in matrix.iter_mut().skip(1) {
        *entry = row;
        row <<= 1;
    }

    square_matrix(&mut square, &matrix);
    square_matrix(&mut matrix, &square);

    let mut crc = left;
    let mut len = right_len;
    loop {
        square_matrix(&mut square, &matrix);
        if len & 1 != 0 {
            crc = matrix_times(&square, crc);
        }
        len >>= 1;
        if len == 0 {
            break;
        }

        square_matrix(&mut matrix, &square);
        if len & 1 != 0 {
            crc = matrix_times(&matrix, crc);
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

    Ok(Part { crc32: crc, len: total })
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
    let hashes = stream.checksum();
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

    let len = parse_hex_u64(bytes, 4, 20).map_err(|offset| VerifyError::Hex { offset })?;
    let crc = parse_hex_u32(bytes, 25, 33).map_err(|offset| VerifyError::Hex { offset })?;
    let adler = parse_hex_u32(bytes, 40, 48).map_err(|offset| VerifyError::Hex { offset })?;

    let mut selected_len = 0u64;
    let mut stream = Stream::new();
    for span in spans {
        let selected = &data[span.start..span.end];
        selected_len += selected.len() as u64;
        stream.update(selected);
    }

    if len != selected_len {
        return Err(VerifyError::Length);
    }

    let actual = stream.checksum();
    if crc != actual.crc32 {
        return Err(VerifyError::Mismatch {
            field: 0,
            expected: crc,
            actual: actual.crc32,
        });
    }
    if adler != actual.adler32 {
        return Err(VerifyError::Mismatch {
            field: 1,
            expected: adler,
            actual: actual.adler32,
        });
    }
    Ok(())
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

fn update_state(crc_raw: &mut u32, a: &mut u32, b: &mut u32, data: &[u8]) {
    for &byte in data {
        *crc_raw ^= byte as u32;
        for _ in 0..8 {
            let low = *crc_raw & 1;
            *crc_raw >>= 1;
            if low != 0 {
                *crc_raw ^= 0xedb8_8320;
            }
        }
        *a = (*a + byte as u32) % 65521;
        *b = (*b + *a) % 65521;
    }
}

fn parse_hex_digit(byte: u8) -> Option<u32> {
    match byte {
        b'0'..=b'9' => Some((byte - b'0') as u32),
        b'a'..=b'f' => Some((byte - b'a' + 10) as u32),
        _ => None,
    }
}

fn parse_hex_u32(bytes: &[u8], start: usize, end: usize) -> Result<u32, usize> {
    let mut value = 0u32;
    for offset in start..end {
        let digit = parse_hex_digit(bytes[offset]).ok_or(offset)?;
        value = (value << 4) | digit;
    }
    Ok(value)
}

fn parse_hex_u64(bytes: &[u8], start: usize, end: usize) -> Result<u64, usize> {
    let mut value = 0u64;
    for offset in start..end {
        let digit = parse_hex_digit(bytes[offset]).ok_or(offset)?;
        value = (value << 4) | digit as u64;
    }
    Ok(value)
}

struct Fletcher16State {
    a: u32,
    b: u32,
}

impl Fletcher16State {
    fn new() -> Self {
        Self { a: 0, b: 0 }
    }

    fn update(&mut self, data: &[u8]) {
        for &byte in data {
            self.a = (self.a + byte as u32) % 255;
            self.b = (self.b + self.a) % 255;
        }
    }

    fn finish(&self) -> u16 {
        ((self.b as u16) << 8) | self.a as u16
    }
}

struct Fletcher32State {
    a: u32,
    b: u32,
    high: Option<u8>,
}

impl Fletcher32State {
    fn new() -> Self {
        Self {
            a: 0,
            b: 0,
            high: None,
        }
    }

    fn word(&mut self, word: u16) {
        self.a = (self.a + word as u32) % 65535;
        self.b = (self.b + self.a) % 65535;
    }

    fn update(&mut self, data: &[u8]) {
        let mut index = 0;
        if let Some(high) = self.high.take() {
            if !data.is_empty() {
                self.word(u16::from_be_bytes([high, data[0]]));
                index = 1;
            } else {
                self.high = Some(high);
                return;
            }
        }

        while index + 1 < data.len() {
            self.word(u16::from_be_bytes([data[index], data[index + 1]]));
            index += 2;
        }

        if index < data.len() {
            self.high = Some(data[index]);
        }
    }

    fn finish(&self) -> u32 {
        let mut a = self.a;
        let mut b = self.b;
        if let Some(high) = self.high {
            a = (a + ((high as u32) << 8)) % 65535;
            b = (b + a) % 65535;
        }
        (b << 16) | a
    }
}

struct Crc16State {
    register: u16,
    variant: Crc16,
}

impl Crc16State {
    fn new(variant: Crc16) -> Self {
        let register = match variant {
            Crc16::Xmodem => 0,
            Crc16::False | Crc16::X25 => 0xffff,
        };
        Self { register, variant }
    }

    fn update(&mut self, data: &[u8]) {
        match self.variant {
            Crc16::Xmodem | Crc16::False => {
                for &byte in data {
                    self.register ^= (byte as u16) << 8;
                    for _ in 0..8 {
                        let top = self.register & 0x8000;
                        self.register = self.register.wrapping_shl(1);
                        if top != 0 {
                            self.register ^= 0x1021;
                        }
                    }
                }
            }
            Crc16::X25 => {
                for &byte in data {
                    self.register ^= byte as u16;
                    for _ in 0..8 {
                        let low = self.register & 1;
                        self.register >>= 1;
                        if low != 0 {
                            self.register ^= 0x8408;
                        }
                    }
                }
            }
        }
    }

    fn finish(&self) -> u16 {
        match self.variant {
            Crc16::X25 => self.register ^ 0xffff,
            Crc16::Xmodem | Crc16::False => self.register,
        }
    }
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

fn square_matrix(square: &mut [u32; 32], matrix: &[u32; 32]) {
    for (index, entry) in square.iter_mut().enumerate() {
        *entry = matrix_times(matrix, matrix[index]);
    }
}

pub fn main() {}