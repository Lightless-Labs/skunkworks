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

#[derive(Debug, Clone)]
pub struct Stream {
    crc: u32,
    a: u32,
    b: u32,
}

impl Default for Stream {
    fn default() -> Self {
        Self::new()
    }
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
                let low = self.crc & 1;
                self.crc >>= 1;
                if low != 0 {
                    self.crc ^= 0xedb8_8320;
                }
            }
            self.a = (self.a + byte as u32) % 65521;
            self.b = (self.b + self.a) % 65521;
        }
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
        let sums = self.checksum();
        format!("c{:08x}:a{:08x}", sums.crc32, sums.adler32)
    }

    pub fn restore(token: &str) -> Result<Self, SnapshotError> {
        let bytes = token.as_bytes();
        if bytes.len() != 19 || bytes[0] != b'c' || bytes[9] != b':' || bytes[10] != b'a' {
            return Err(SnapshotError::Shape);
        }

        let mut crc = 0u32;
        let mut adler = 0u32;
        for (offset, &byte) in bytes.iter().enumerate() {
            if (1..=8).contains(&offset) || (11..=18).contains(&offset) {
                let digit = match byte {
                    b'0'..=b'9' => (byte - b'0') as u32,
                    b'a'..=b'f' => (byte - b'a' + 10) as u32,
                    _ => return Err(SnapshotError::Hex { offset }),
                };
                if offset <= 8 {
                    crc = (crc << 4) | digit;
                } else {
                    adler = (adler << 4) | digit;
                }
            }
        }
        Self::from_seed(crc, adler)
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

fn adler_update(a: &mut u32, b: &mut u32, data: &[u8]) {
    for &byte in data {
        *a = (*a + byte as u32) % 65521;
        *b = (*b + *a) % 65521;
    }
}

pub fn crc32(data: &[u8]) -> u32 {
    crc32_update(0xffff_ffff, data) ^ 0xffff_ffff
}

pub fn adler32(data: &[u8]) -> u32 {
    let mut a = 1;
    let mut b = 0;
    adler_update(&mut a, &mut b, data);
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
    ((b << 8) | a) as u16
}

#[derive(Debug, Clone, Copy)]
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
        for &byte in data {
            if let Some(high) = self.pending.take() {
                let word = ((high as u32) << 8) | byte as u32;
                self.add_word(word);
            } else {
                self.pending = Some(byte);
            }
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
    for span in spans {
        state.update(&data[span.start..span.end]);
    }
    Ok(state.finish())
}

pub fn crc16(data: &[u8], variant: Crc16) -> u16 {
    let mut crc: u16 = match variant {
        Crc16::Xmodem => 0,
        Crc16::False | Crc16::X25 => 0xffff,
    };

    for &byte in data {
        match variant {
            Crc16::Xmodem | Crc16::False => {
                crc ^= (byte as u16) << 8;
                for _ in 0..8 {
                    let top = crc & 0x8000;
                    crc = crc.wrapping_shl(1);
                    if top != 0 {
                        crc ^= 0x1021;
                    }
                }
            }
            Crc16::X25 => {
                crc ^= byte as u16;
                for _ in 0..8 {
                    let low = crc & 1;
                    crc >>= 1;
                    if low != 0 {
                        crc ^= 0x8408;
                    }
                }
            }
        }
    }

    if variant == Crc16::X25 {
        crc ^ 0xffff
    } else {
        crc
    }
}

pub fn multi_checksums(
    data: &[u8],
    spans: &[Span],
    variant: Crc16,
) -> Result<Multi, SpanError> {
    validate_spans(data.len(), spans)?;

    let mut base = Stream::new();
    let mut f16_a = 0u32;
    let mut f16_b = 0u32;
    let mut f32 = Fletcher32State::new();
    let mut c16 = match variant {
        Crc16::Xmodem => 0u16,
        Crc16::False | Crc16::X25 => 0xffff,
    };

    for span in spans {
        let selected = &data[span.start..span.end];
        base.update(selected);
        f32.update(selected);

        for &byte in selected {
            f16_a = (f16_a + byte as u32) % 255;
            f16_b = (f16_b + f16_a) % 255;

            match variant {
                Crc16::Xmodem | Crc16::False => {
                    c16 ^= (byte as u16) << 8;
                    for _ in 0..8 {
                        let top = c16 & 0x8000;
                        c16 = c16.wrapping_shl(1);
                        if top != 0 {
                            c16 ^= 0x1021;
                        }
                    }
                }
                Crc16::X25 => {
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
        base: base.checksum(),
        fletcher16: ((f16_b << 8) | f16_a) as u16,
        fletcher32: f32.finish(),
        crc16: c16,
    })
}
