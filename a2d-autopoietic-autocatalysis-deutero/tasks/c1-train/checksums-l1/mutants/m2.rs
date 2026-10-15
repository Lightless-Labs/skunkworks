
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
    fn value(&self) -> Hashes { Hashes { crc32: self.crc, adler32: (self.b << 16) | self.a } }
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
