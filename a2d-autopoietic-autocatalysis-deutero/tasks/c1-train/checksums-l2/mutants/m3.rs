
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
    pub fn reset(&mut self) { self.state.crc = 0xffffffff; }
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
