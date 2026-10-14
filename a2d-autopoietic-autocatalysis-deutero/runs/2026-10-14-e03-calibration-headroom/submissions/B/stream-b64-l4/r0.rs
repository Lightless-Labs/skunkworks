#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alphabet {
    Standard,
    UrlSafe,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Padding {
    Required,
    Forbidden,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    pub alphabet: Alphabet,
    pub padding: Padding,
    pub ignore_whitespace: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    InvalidByte { offset: usize },
    InvalidPadding { offset: usize },
    InvalidLength { offset: usize },
    TrailingData { offset: usize },
    NonCanonical { offset: usize },
    Finished,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamError {
    Finished,
}

fn encode_sextet(value: u8, alphabet: Alphabet) -> u8 {
    match value {
        0..=25 => b'A' + value,
        26..=51 => b'a' + (value - 26),
        52..=61 => b'0' + (value - 52),
        62 => match alphabet {
            Alphabet::Standard => b'+',
            Alphabet::UrlSafe => b'-',
        },
        _ => match alphabet {
            Alphabet::Standard => b'/',
            Alphabet::UrlSafe => b'_',
        },
    }
}

fn decode_sextet(byte: u8, alphabet: Alphabet) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' if alphabet == Alphabet::Standard => Some(62),
        b'/' if alphabet == Alphabet::Standard => Some(63),
        b'-' if alphabet == Alphabet::UrlSafe => Some(62),
        b'_' if alphabet == Alphabet::UrlSafe => Some(63),
        _ => None,
    }
}

fn is_ignored_whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

fn encode_triplet(a: u8, b: u8, c: u8, alphabet: Alphabet, out: &mut String) {
    let values = [
        a >> 2,
        ((a & 0x03) << 4) | (b >> 4),
        ((b & 0x0f) << 2) | (c >> 6),
        c & 0x3f,
    ];
    for value in values {
        out.push(encode_sextet(value, alphabet) as char);
    }
}

fn encode_tail(tail: &[u8], options: Options) -> String {
    let mut out = String::new();
    match tail {
        [] => {}
        [a] => {
            out.push(encode_sextet(a >> 2, options.alphabet) as char);
            out.push(encode_sextet((a & 0x03) << 4, options.alphabet) as char);
            if options.padding == Padding::Required {
                out.push_str("==");
            }
        }
        [a, b] => {
            out.push(encode_sextet(a >> 2, options.alphabet) as char);
            out.push(encode_sextet(((a & 0x03) << 4) | (b >> 4), options.alphabet) as char);
            out.push(encode_sextet((b & 0x0f) << 2, options.alphabet) as char);
            if options.padding == Padding::Required {
                out.push('=');
            }
        }
        _ => unreachable!(),
    }
    out
}

pub fn encode(input: &[u8], options: Options) -> String {
    let mut out = String::with_capacity(input.len().saturating_add(2) / 3 * 4);
    let mut chunks = input.chunks_exact(3);
    for chunk in &mut chunks {
        encode_triplet(chunk[0], chunk[1], chunk[2], options.alphabet, &mut out);
    }
    out.push_str(&encode_tail(chunks.remainder(), options));
    out
}

pub fn decode(input: &str, options: Options) -> Result<Vec<u8>, DecodeError> {
    let mut decoder = Decoder::new(options);
    let mut out = decoder.push(input)?;
    out.extend(decoder.finish()?);
    Ok(out)
}

pub struct Encoder {
    options: Options,
    tail: [u8; 2],
    tail_len: usize,
    finished: bool,
}

impl Encoder {
    pub fn new(options: Options) -> Self {
        Self {
            options,
            tail: [0; 2],
            tail_len: 0,
            finished: false,
        }
    }

    pub fn push(&mut self, input: &[u8]) -> Result<String, StreamError> {
        if self.finished {
            return Err(StreamError::Finished);
        }

        let mut out = String::new();
        let mut index = 0;

        if self.tail_len > 0 {
            while self.tail_len < 3 && index < input.len() {
                if self.tail_len < 2 {
                    self.tail[self.tail_len] = input[index];
                }
                self.tail_len += 1;
                index += 1;
            }
            if self.tail_len == 3 {
                encode_triplet(self.tail[0], self.tail[1], input[index - 1], self.options.alphabet, &mut out);
                self.tail_len = 0;
            }
        }

        while index + 3 <= input.len() {
            encode_triplet(
                input[index],
                input[index + 1],
                input[index + 2],
                self.options.alphabet,
                &mut out,
            );
            index += 3;
        }

        while index < input.len() {
            self.tail[self.tail_len] = input[index];
            self.tail_len += 1;
            index += 1;
        }

        Ok(out)
    }

    pub fn finish(&mut self) -> Result<String, StreamError> {
        if self.finished {
            return Err(StreamError::Finished);
        }
        self.finished = true;
        let out = encode_tail(&self.tail[..self.tail_len], self.options);
        self.tail_len = 0;
        Ok(out)
    }
}

#[derive(Clone, Copy)]
struct Symbol {
    value: u8,
    offset: usize,
}

pub struct Decoder {
    options: Options,
    symbols: [Symbol; 4],
    len: usize,
    pending_double_padding: bool,
    terminated: bool,
    offset: usize,
    finished: bool,
    error: Option<DecodeError>,
}

impl Decoder {
    pub fn new(options: Options) -> Self {
        Self {
            options,
            symbols: [Symbol { value: 0, offset: 0 }; 4],
            len: 0,
            pending_double_padding: false,
            terminated: false,
            offset: 0,
            finished: false,
            error: None,
        }
    }

    fn fail(&mut self, error: DecodeError) -> DecodeError {
        self.error = Some(error);
        error
    }

    fn process_byte(&mut self, byte: u8, offset: usize, output: &mut Vec<u8>) -> Result<(), DecodeError> {
        if self.terminated {
            return Err(self.fail(DecodeError::TrailingData { offset }));
        }

        let pos = self.len;
        if byte == b'=' {
            if self.options.padding == Padding::Forbidden || pos < 2 {
                return Err(self.fail(DecodeError::InvalidPadding { offset }));
            }

            if self.pending_double_padding {
                if pos != 3 {
                    return Err(self.fail(DecodeError::InvalidPadding { offset }));
                }
                self.symbols[3] = Symbol { value: 64, offset };
                self.len = 4;
                return self.complete_quartet(output);
            }

            if pos == 2 {
                self.symbols[2] = Symbol { value: 64, offset };
                self.len = 3;
                self.pending_double_padding = true;
                return Ok(());
            }

            if pos == 3 {
                self.symbols[3] = Symbol { value: 64, offset };
                self.len = 4;
                return self.complete_quartet(output);
            }

            return Err(self.fail(DecodeError::InvalidPadding { offset }));
        }

        if self.pending_double_padding {
            return Err(self.fail(DecodeError::InvalidPadding { offset }));
        }

        let value = match decode_sextet(byte, self.options.alphabet) {
            Some(value) => value,
            None => return Err(self.fail(DecodeError::InvalidByte { offset })),
        };

        self.symbols[pos] = Symbol { value, offset };
        self.len += 1;
        if self.len == 4 {
            self.complete_quartet(output)?;
        }
        Ok(())
    }

    fn complete_quartet(&mut self, output: &mut Vec<u8>) -> Result<(), DecodeError> {
        let a = self.symbols[0].value;
        let b = self.symbols[1].value;
        let c = self.symbols[2].value;
        let d = self.symbols[3].value;

        if c == 64 {
            if b & 0x0f != 0 {
                return Err(self.fail(DecodeError::NonCanonical {
                    offset: self.symbols[1].offset,
                }));
            }
            output.push((a << 2) | (b >> 4));
            self.terminated = true;
        } else if d == 64 {
            if c & 0x03 != 0 {
                return Err(self.fail(DecodeError::NonCanonical {
                    offset: self.symbols[2].offset,
                }));
            }
            output.push((a << 2) | (b >> 4));
            output.push((b << 4) | (c >> 2));
            self.terminated = true;
        } else {
            output.push((a << 2) | (b >> 4));
            output.push((b << 4) | (c >> 2));
            output.push((c << 6) | d);
        }

        self.len = 0;
        self.pending_double_padding = false;
        Ok(())
    }

    pub fn push(&mut self, input: &str) -> Result<Vec<u8>, DecodeError> {
        if let Some(error) = self.error {
            return Err(error);
        }
        if self.finished {
            return Err(DecodeError::Finished);
        }

        let mut output = Vec::new();
        for &byte in input.as_bytes() {
            let current_offset = self.offset;
            self.offset += 1;

            if self.options.ignore_whitespace && is_ignored_whitespace(byte) {
                continue;
            }

            if let Err(error) = self.process_byte(byte, current_offset, &mut output) {
                return Err(error);
            }
        }
        Ok(output)
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, DecodeError> {
        if let Some(error) = self.error {
            return Err(error);
        }
        if self.finished {
            return Err(DecodeError::Finished);
        }

        let output = Vec::new();
        if self.options.padding == Padding::Required {
            if self.len != 0 {
                return Err(self.fail(DecodeError::InvalidLength { offset: self.offset }));
            }
        } else {
            match self.len {
                0 => {}
                1 => return Err(self.fail(DecodeError::InvalidLength { offset: self.offset })),
                2 => {
                    let b = self.symbols[1];
                    if b.value & 0x0f != 0 {
                        return Err(self.fail(DecodeError::NonCanonical { offset: b.offset }));
                    }
                }
                3 => {
                    let c = self.symbols[2];
                    if c.value & 0x03 != 0 {
                        return Err(self.fail(DecodeError::NonCanonical { offset: c.offset }));
                    }
                }
                _ => unreachable!(),
            }
        }

        let mut output = output;
        match self.len {
            2 => {
                let a = self.symbols[0].value;
                let b = self.symbols[1].value;
                output.push((a << 2) | (b >> 4));
            }
            3 => {
                let a = self.symbols[0].value;
                let b = self.symbols[1].value;
                let c = self.symbols[2].value;
                output.push((a << 2) | (b >> 4));
                output.push((b << 4) | (c >> 2));
            }
            _ => {}
        }

        self.finished = true;
        Ok(output)
    }
}