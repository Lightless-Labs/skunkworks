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

const ADLER_MOD: u32 = 65_521;

fn crc32_update(mut crc: u32, data: &[u8]) -> u32 {
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            let low = crc & 1;
            crc >>= 1;
            if low != 0 {
                crc ^= 0xedb8_8320;
            }
        }
    }
    crc
}

fn adler_update(mut a: u32, mut b: u32, data: &[u8]) -> (u32, u32) {
    for &byte in data {
        a = (a + byte as u32) % ADLER_MOD;
        b = (b + a) % ADLER_MOD;
    }
    (a, b)
}

pub fn crc32(data: &[u8]) -> u32 {
    crc32_update(0xffff_ffff, data) ^ 0xffff_ffff
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

    pub fn checksum(&self) -> Hashes {
        Hashes {
            crc32: self.crc ^ 0xffff_ffff,
            adler32: (self.b << 16) | self.a,
        }
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

        let mut values = [0u32; 2];
        for (number, range) in [(0usize, 1usize..9usize), (1usize, 11usize..19usize)] {
            let mut value = 0u32;
            for offset in range {
                let digit = match bytes[offset] {
                    b'0'..=b'9' => (bytes[offset] - b'0') as u32,
                    b'a'..=b'f' => (bytes[offset] - b'a' + 10) as u32,
                    _ => return Err(SnapshotError::Hex { offset }),
                };
                value = (value << 4) | digit;
            }
            values[number] = value;
        }

        Self::from_seed(values[0], values[1])
    }
}

impl Default for Stream {
    fn default() -> Self {
        Self::new()
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
    let (mut a, mut b) = (0u32, 0u32);
    for &byte in data {
        a = (a + byte as u32) % 255;
        b = (b + a) % 255;
    }
    ((b as u16) << 8) | a as u16
}

fn fletcher32_feed(a: &mut u32, b: &mut u32, word: u16) {
    *a = (*a + word as u32) % 65_535;
    *b = (*b + *a) % 65_535;
}

fn fletcher32_selected(data: &[u8], spans: &[Span]) -> u32 {
    let (mut a, mut b) = (0u32, 0u32);
    let mut high: Option<u8> = None;

    for span in spans {
        for &byte in &data[span.start..span.end] {
            if let Some(first) = high.take() {
                fletcher32_feed(&mut a, &mut b, u16::from_be_bytes([first, byte]));
            } else {
                high = Some(byte);
            }
        }
    }

    if let Some(first) = high {
        fletcher32_feed(&mut a, &mut b, u16::from_be_bytes([first, 0]));
    }

    (b << 16) | a
}

pub fn fletcher32(data: &[u8]) -> u32 {
    fletcher32_selected(
        data,
        &[Span {
            start: 0,
            end: data.len(),
        }],
    )
}

pub fn fletcher32_spans(data: &[u8], spans: &[Span]) -> Result<u32, SpanError> {
    validate_spans(data.len(), spans)?;
    Ok(fletcher32_selected(data, spans))
}

pub fn crc16(data: &[u8], variant: Crc16) -> u16 {
    match variant {
        Crc16::Xmodem | Crc16::False => {
            let mut crc: u16 = if variant == Crc16::Xmodem { 0 } else { 0xffff };
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
            let mut crc: u16 = 0xffff;
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
    let mut pending_high: Option<u8> = None;

    let mut crc16_value: u16 = match variant {
        Crc16::Xmodem => 0,
        Crc16::False | Crc16::X25 => 0xffff,
    };

    for span in spans {
        let selected = &data[span.start..span.end];
        stream.update(selected);

        for &byte in selected {
            f16_a = (f16_a + byte as u32) % 255;
            f16_b = (f16_b + f16_a) % 255;

            if let Some(first) = pending_high.take() {
                fletcher32_feed(&mut f32_a, &mut f32_b, u16::from_be_bytes([first, byte]));
            } else {
                pending_high = Some(byte);
            }

            match variant {
                Crc16::Xmodem | Crc16::False => {
                    crc16_value ^= (byte as u16) << 8;
                    for _ in 0..8 {
                        let top = crc16_value & 0x8000;
                        crc16_value = crc16_value.wrapping_shl(1);
                        if top != 0 {
                            crc16_value ^= 0x1021;
                        }
                    }
                }
                Crc16::X25 => {
                    crc16_value ^= byte as u16;
                    for _ in 0..8 {
                        let low = crc16_value & 1;
                        crc16_value >>= 1;
                        if low != 0 {
                            crc16_value ^= 0x8408;
                        }
                    }
                }
            }
        }
    }

    if let Some(first) = pending_high {
        fletcher32_feed(
            &mut f32_a,
            &mut f32_b,
            u16::from_be_bytes([first, 0]),
        );
    }

    if variant == Crc16::X25 {
        crc16_value ^= 0xffff;
    }

    Ok(Multi {
        base: stream.checksum(),
        fletcher16: ((f16_b as u16) << 8) | f16_a as u16,
        fletcher32: (f32_b << 16) | f32_a,
        crc16: crc16_value,
    })
}

fn main() {}