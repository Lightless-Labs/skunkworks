
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span { pub start: usize, pub end: usize }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hashes { pub crc32: u32, pub adler32: u32 }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpanError {
    Reversed { span: usize },
    OutOfBounds { span: usize, offset: usize },
    Overlap { span: usize },
}
fn validate(data: &[u8], spans: &[Span]) -> Result<(), SpanError> {
    for (i, span) in spans.iter().enumerate() {
        if span.start > span.end { return Err(SpanError::Reversed { span: i }); }
        if span.start > data.len() { return Err(SpanError::OutOfBounds { span: i, offset: span.start }); }
        if span.end > data.len() { return Err(SpanError::OutOfBounds { span: i, offset: span.end }); }
        if i > 0 && span.start < spans[i-1].end { return Err(SpanError::Overlap { span: i }); }
    }
    Ok(())
}
#[derive(Debug, Clone)]
struct Acc { crc: u32, a: u32, b: u32 }
impl Acc {
    fn new() -> Self { Self { crc: 0xffffffff, a: 1, b: 0 } }
    fn byte(&mut self, x: u8) {
        self.crc ^= x as u32;
        for _ in 0..8 {
            self.crc = (self.crc >> 1) ^ if self.crc & 1 != 0 { 0xedb88320 } else { 0 };
        }
        self.a = (self.a + x as u32) % 65521;
        self.b = (self.b + self.a) % 65521;
    }
    fn update(&mut self, data: &[u8]) { for &x in data { self.byte(x); } }
    fn value(&self) -> Hashes { Hashes { crc32: self.crc ^ 0xffffffff, adler32: (self.b << 16) | self.a } }
}
pub fn crc32(data: &[u8]) -> u32 { checksums(data).crc32 }
pub fn adler32(data: &[u8]) -> u32 { checksums(data).adler32 }
pub fn checksums(data: &[u8]) -> Hashes {
    let mut a = Acc::new(); a.update(data); a.value()
}
pub fn checksums_spans(data: &[u8], spans: &[Span]) -> Result<Hashes, SpanError> {
    validate(data, spans)?;
    let mut a = Acc::new();
    for s in spans { a.update(&data[s.start..s.end]); }
    Ok(a.value())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotError { Shape, Hex { offset: usize }, Adler { component: usize } }
#[derive(Debug, Clone)]
pub struct Stream { state: Acc }
fn hex_number(bytes: &[u8], start: usize, end: usize) -> Result<u64, usize> {
    let mut n = 0u64;
    for i in start..end {
        let d = match bytes[i] {
            b'0'..=b'9' => bytes[i] - b'0',
            b'a'..=b'f' => bytes[i] - b'a' + 10,
            _ => return Err(i),
        };
        n = (n << 4) | d as u64;
    }
    Ok(n)
}
impl Stream {
    pub fn new() -> Self { Self { state: Acc::new() } }
    pub fn from_seed(crc32: u32, adler32: u32) -> Result<Self, SnapshotError> {
        let a = adler32 & 0xffff;
        let b = adler32 >> 16;
        if a >= 65521 { return Err(SnapshotError::Adler { component: 0 }); }
        if b >= 65521 { return Err(SnapshotError::Adler { component: 1 }); }
        Ok(Self { state: Acc { crc: crc32 ^ 0xffffffff, a, b } })
    }
    pub fn checksum(&self) -> Hashes { self.state.value() }
    pub fn update(&mut self, data: &[u8]) { self.state.update(data); }
    pub fn update_spans(&mut self, data: &[u8], spans: &[Span]) -> Result<(), SpanError> {
        validate(data, spans)?;
        for s in spans { self.update(&data[s.start..s.end]); }
        Ok(())
    }
    pub fn reset(&mut self) { *self = Self::new(); }
    pub fn snapshot(&self) -> String {
        let h = self.checksum(); format!("c{:08x}:a{:08x}", h.crc32, h.adler32)
    }
    pub fn restore(token: &str) -> Result<Self, SnapshotError> {
        let b = token.as_bytes();
        if b.len() != 19 || b[0] != b'c' || b[9] != b':' || b[10] != b'a' { return Err(SnapshotError::Shape); }
        let crc = hex_number(b, 1, 9).map_err(|offset| SnapshotError::Hex { offset })? as u32;
        let adler = hex_number(b, 11, 19).map_err(|offset| SnapshotError::Hex { offset })? as u32;
        Self::from_seed(crc, adler)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Crc16 { Xmodem, False, X25 }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Multi { pub base: Hashes, pub fletcher16: u16, pub fletcher32: u32, pub crc16: u16 }
#[derive(Clone)]
struct Extra { a16: u32, b16: u32, a32: u32, b32: u32, pending: Option<u8>, crc: u16, variant: Crc16 }
impl Extra {
    fn new(variant: Crc16) -> Self {
        Self { a16: 0, b16: 0, a32: 0, b32: 0, pending: None,
            crc: if variant == Crc16::Xmodem { 0 } else { 0xffff }, variant }
    }
    fn word(&mut self, w: u16) {
        self.a32 = (self.a32 + w as u32) % 65535;
        self.b32 = (self.b32 + self.a32) % 65535;
    }
    fn byte(&mut self, x: u8) {
        self.a16 = (self.a16 + x as u32) % 255;
        self.b16 = (self.b16 + self.a16) % 255;
        if let Some(hi) = self.pending.take() { self.word(((hi as u16) << 8) | x as u16); }
        else { self.pending = Some(x); }
        if self.variant == Crc16::X25 {
            self.crc ^= x as u16;
            for _ in 0..8 { self.crc = (self.crc >> 1) ^ if self.crc & 1 != 0 { 0x8408 } else { 0 }; }
        } else {
            self.crc ^= (x as u16) << 8;
            for _ in 0..8 { self.crc = (self.crc << 1) ^ if self.crc & 0x8000 != 0 { 0x1021 } else { 0 }; }
        }
    }
    fn finish(mut self) -> (u16, u32, u16) {
        if let Some(hi) = self.pending.take() { self.word((hi as u16) << 8); }
        (((self.b16 << 8) | self.a16) as u16, (self.b32 << 16) | self.a32,
            self.crc ^ if self.variant == Crc16::X25 { 0xffff } else { 0 })
    }
}
pub fn fletcher16(data: &[u8]) -> u16 {
    let mut e = Extra::new(Crc16::Xmodem); for &x in data { e.byte(x); } e.finish().0
}
pub fn fletcher32(data: &[u8]) -> u32 {
    let mut e = Extra::new(Crc16::Xmodem); for &x in data { e.byte(x); } e.finish().1
}
pub fn crc16(data: &[u8], variant: Crc16) -> u16 {
    let mut e = Extra::new(variant); for &x in data { e.byte(x); } e.finish().2
}
pub fn fletcher32_spans(data: &[u8], spans: &[Span]) -> Result<u32, SpanError> {
    Ok(multi_checksums(data, spans, Crc16::Xmodem)?.fletcher32)
}
pub fn multi_checksums(data: &[u8], spans: &[Span], variant: Crc16) -> Result<Multi, SpanError> {
    validate(data, spans)?;
    let mut a = Acc::new(); let mut e = Extra::new(variant);
    for s in spans { for &x in &data[s.start..s.end] { a.byte(x); e.byte(x); } }
    let (fletcher16, fletcher32, crc16) = e.finish();
    Ok(Multi { base: a.value(), fletcher16, fletcher32, crc16 })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Part { pub crc32: u32, pub len: u64 }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CombineError { LengthOverflow { part: usize }, EmptyCrc { part: usize } }
fn matrix_times(m: &[u32; 32], mut v: u32) -> u32 {
    let mut result = 0; let mut i = 0;
    while v != 0 { if v & 1 != 0 { result ^= m[i]; } v >>= 1; i += 1; }
    result
}
pub fn crc32_combine(left: u32, right: u32, right_len: u64) -> u32 {
    if right_len == 0 { return left; }
    let mut op = [0u32; 32];
    for i in 0..32 {
        let mut r = 1u32 << i;
        for _ in 0..8 { r = (r >> 1) ^ if r & 1 != 0 { 0xedb88320 } else { 0 }; }
        op[i] = r;
    }
    let mut n = right_len; let mut value = left;
    while n != 0 {
        if n & 1 != 0 { value = matrix_times(&op, value); }
        n >>= 1;
        if n != 0 {
            let old = op;
            for i in 0..32 { op[i] = matrix_times(&old, old[i]); }
        }
    }
    value ^ right
}
pub fn combine_parts(parts: &[Part]) -> Result<Part, CombineError> {
    let mut total = 0u64;
    for (i, p) in parts.iter().enumerate() {
        total = total.checked_add(p.len).ok_or(CombineError::LengthOverflow { part: i })?;
    }
    for (i, p) in parts.iter().enumerate() {
        if p.len == 0 && p.crc32 != 0 { return Err(CombineError::EmptyCrc { part: i }); }
    }
    let mut crc = 0;
    for p in parts { crc = crc32_combine(crc, p.crc32, p.len); }
    Ok(Part { crc32: crc, len: total })
}
fn selected_len(spans: &[Span]) -> u64 { spans.iter().map(|s| (s.end-s.start) as u64).sum() }
pub fn manifest(data: &[u8], spans: &[Span]) -> Result<String, SpanError> {
    let h = checksums_spans(data, spans)?;
    Ok(format!("len={:016x};crc={:08x};adler={:08x}", selected_len(spans), h.crc32, h.adler32))
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifyError {
    Span(SpanError), Shape, Hex { offset: usize }, Length,
    Mismatch { field: usize, expected: u32, actual: u32 },
}
pub fn verify(data: &[u8], spans: &[Span], token: &str) -> Result<(), VerifyError> {
    validate(data, spans).map_err(VerifyError::Span)?;
    let b = token.as_bytes();
    if b.len() != 48 || &b[0..4] != b"len=" || &b[20..25] != b";crc=" || &b[33..40] != b";adler=" { return Err(VerifyError::Shape); }
    let len = hex_number(b, 4, 20).map_err(|offset| VerifyError::Hex { offset })?;
    let crc = hex_number(b, 25, 33).map_err(|offset| VerifyError::Hex { offset })? as u32;
    let adler = hex_number(b, 40, 48).map_err(|offset| VerifyError::Hex { offset })? as u32;
    if len != selected_len(spans) { return Err(VerifyError::Length); }
    let actual = checksums_spans(data, spans).map_err(VerifyError::Span)?;
    if crc != actual.crc32 { return Err(VerifyError::Mismatch { field: 0, expected: crc, actual: actual.crc32 }); }
    if adler != actual.adler32 { return Err(VerifyError::Mismatch { field: 1, expected: adler, actual: actual.adler32 }); }
    Ok(())
}
