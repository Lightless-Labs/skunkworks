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

const ADLER_MOD: u32 = 65_521;

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
        for &byte in data {
            self.update_byte(byte);
        }
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

        let mut values = [0u32; 2];
        for (index, offset) in (1..9).chain(11..19).enumerate() {
            let digit = match bytes[offset] {
                b'0'..=b'9' => (bytes[offset] - b'0') as u32,
                b'a'..=b'f' => (bytes[offset] - b'a' + 10) as u32,
                _ => return Err(SnapshotError::Hex { offset }),
            };
            values[index / 8] = (values[index / 8] << 4) | digit;
        }

        Self::from_seed(values[0], values[1])
    }

    fn update_byte(&mut self, byte: u8) {
        self.crc ^= byte as u32;
        for _ in 0..8 {
            let low = self.crc & 1;
            self.crc >>= 1;
            if low != 0 {
                self.crc ^= 0xedb8_8320;
            }
        }

        self.adler_a = (self.adler_a + byte as u32) % ADLER_MOD;
        self.adler_b = (self.adler_b + self.adler_a) % ADLER_MOD;
    }
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

pub fn crc32(data: &[u8]) -> u32 {
    let mut stream = Stream::new();
    stream.update(data);
    stream.checksum().crc32
}

pub fn adler32(data: &[u8]) -> u32 {
    let mut stream = Stream::new();
    stream.update(data);
    stream.checksum().adler32
}

pub fn checksums(data: &[u8]) -> Hashes {
    let mut stream = Stream::new();
    stream.update(data);
    stream.checksum()
}

pub fn checksums_spans(data: &[u8], spans: &[Span]) -> Result<Hashes, SpanError> {
    let mut stream = Stream::new();
    stream.update_spans(data, spans)?;
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

    fn update_byte(&mut self, byte: u8) {
        if let Some(high) = self.pending.take() {
            self.update_word(((high as u16) << 8) | byte as u16);
        } else {
            self.pending = Some(byte);
        }
    }

    fn update_word(&mut self, word: u16) {
        self.a = (self.a + word as u32) % 65_535;
        self.b = (self.b + self.a) % 65_535;
    }

    fn finish(mut self) -> u32 {
        if let Some(high) = self.pending.take() {
            self.update_word((high as u16) << 8);
        }
        (self.b << 16) | self.a
    }
}

pub fn fletcher32(data: &[u8]) -> u32 {
    let mut state = Fletcher32State::new();
    for &byte in data {
        state.update_byte(byte);
    }
    state.finish()
}

pub fn fletcher32_spans(data: &[u8], spans: &[Span]) -> Result<u32, SpanError> {
    validate_spans(data.len(), spans)?;
    let mut state = Fletcher32State::new();
    for span in spans {
        for &byte in &data[span.start..span.end] {
            state.update_byte(byte);
        }
    }
    Ok(state.finish())
}

fn crc16_update(mut register: u16, byte: u8, variant: Crc16) -> u16 {
    match variant {
        Crc16::Xmodem | Crc16::False => {
            register ^= (byte as u16) << 8;
            for _ in 0..8 {
                let top = register & 0x8000;
                register = register.wrapping_shl(1);
                if top != 0 {
                    register ^= 0x1021;
                }
            }
        }
        Crc16::X25 => {
            register ^= byte as u16;
            for _ in 0..8 {
                let low = register & 1;
                register >>= 1;
                if low != 0 {
                    register ^= 0x8408;
                }
            }
        }
    }
    register
}

pub fn crc16(data: &[u8], variant: Crc16) -> u16 {
    let mut register = match variant {
        Crc16::Xmodem => 0,
        Crc16::False | Crc16::X25 => 0xffff,
    };
    for &byte in data {
        register = crc16_update(register, byte, variant);
    }
    if variant == Crc16::X25 {
        register ^ 0xffff
    } else {
        register
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Multi {
    pub base: Hashes,
    pub fletcher16: u16,
    pub fletcher32: u32,
    pub crc16: u16,
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
    let mut f32 = Fletcher32State::new();
    let mut crc16_register = match variant {
        Crc16::Xmodem => 0,
        Crc16::False | Crc16::X25 => 0xffff,
    };

    for span in spans {
        for &byte in &data[span.start..span.end] {
            stream.update_byte(byte);

            f16_a = (f16_a + byte as u32) % 255;
            f16_b = (f16_b + f16_a) % 255;

            f32.update_byte(byte);
            crc16_register = crc16_update(crc16_register, byte, variant);
        }
    }

    if variant == Crc16::X25 {
        crc16_register ^= 0xffff;
    }

    Ok(Multi {
        base: stream.checksum(),
        fletcher16: ((f16_b as u16) << 8) | f16_a as u16,
        fletcher32: f32.finish(),
        crc16: crc16_register,
    })
}