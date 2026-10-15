#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alphabet {
    Standard,
    UrlSafe,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Padding {
    Required,
    Forbidden,
    Optional,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    Lf,
    CrLf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Whitespace {
    None,
    IgnoreAscii,
    Wrapped { width: usize, ending: LineEnding },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    pub alphabet: Alphabet,
    pub padding: Padding,
    pub whitespace: Whitespace,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    InvalidByte { offset: usize, byte: u8 },
    BadPadding { offset: usize },
    Truncated { offset: usize },
    AlreadyFinished,
    InvalidWidth { width: usize },
    BadWrap { offset: usize },
}

fn validate_options(options: Options) -> Result<(), Error> {
    if let Whitespace::Wrapped { width, .. } = options.whitespace {
        if !(4..=120).contains(&width) || width % 4 != 0 {
            return Err(Error::InvalidWidth { width });
        }
    }
    Ok(())
}

fn encode_digit(value: u8, alphabet: Alphabet) -> u8 {
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

fn decode_digit(byte: u8, alphabet: Alphabet) -> Option<u8> {
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

fn encode_group(bytes: &[u8], alphabet: Alphabet, padding: Padding) -> String {
    let mut out = String::with_capacity(4);
    let a = bytes[0];
    let b = if bytes.len() > 1 { bytes[1] } else { 0 };
    let c = if bytes.len() > 2 { bytes[2] } else { 0 };

    out.push(encode_digit(a >> 2, alphabet) as char);
    out.push(encode_digit(((a & 0x03) << 4) | (b >> 4), alphabet) as char);

    if bytes.len() > 1 {
        out.push(encode_digit(((b & 0x0f) << 2) | (c >> 6), alphabet) as char);
    } else if padding != Padding::Forbidden {
        out.push('=');
    }

    if bytes.len() > 2 {
        out.push(encode_digit(c & 0x3f, alphabet) as char);
    } else if padding != Padding::Forbidden {
        out.push('=');
    }

    out
}

fn apply_wrapping(raw: String, whitespace: Whitespace) -> String {
    let Whitespace::Wrapped { width, ending } = whitespace else {
        return raw;
    };

    let separator = match ending {
        LineEnding::Lf => "\n",
        LineEnding::CrLf => "\r\n",
    };
    let mut result = String::with_capacity(raw.len() + raw.len() / width * separator.len());
    for (index, byte) in raw.bytes().enumerate() {
        if index != 0 && index % width == 0 {
            result.push_str(separator);
        }
        result.push(byte as char);
    }
    result
}

pub fn encode(input: &[u8], options: Options) -> Result<String, Error> {
    validate_options(options)?;
    let mut raw = String::new();
    for chunk in input.chunks(3) {
        raw.push_str(&encode_group(chunk, options.alphabet, options.padding));
    }
    Ok(apply_wrapping(raw, options.whitespace))
}

#[derive(Clone, Copy)]
struct Symbol {
    byte: u8,
    offset: usize,
}

pub struct Encoder {
    options: Options,
    tail: Vec<u8>,
    closed: bool,
}

impl Encoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate_options(options)?;
        Ok(Self {
            options,
            tail: Vec::new(),
            closed: false,
        })
    }

    pub fn push(&mut self, input: &[u8]) -> Result<String, Error> {
        if self.closed {
            return Err(Error::AlreadyFinished);
        }
        let mut bytes = Vec::with_capacity(self.tail.len() + input.len());
        bytes.extend_from_slice(&self.tail);
        bytes.extend_from_slice(input);

        let complete_len = bytes.len() / 3 * 3;
        let mut raw = String::new();
        for chunk in bytes[..complete_len].chunks(3) {
            raw.push_str(&encode_group(chunk, self.options.alphabet, self.options.padding));
        }

        self.tail.clear();
        self.tail.extend_from_slice(&bytes[complete_len..]);
        Ok(apply_wrapping(raw, self.options.whitespace))
    }

    pub fn finish(&mut self) -> Result<String, Error> {
        if self.closed {
            return Err(Error::AlreadyFinished);
        }
        self.closed = true;
        let raw = if self.tail.is_empty() {
            String::new()
        } else {
            encode_group(&self.tail, self.options.alphabet, self.options.padding)
        };
        self.tail.clear();
        Ok(apply_wrapping(raw, self.options.whitespace))
    }
}

#[derive(Clone, Copy)]
enum WrapState {
    Row,
    NeedSeparator { first_offset: usize, got_cr: bool },
    NeedSymbol { first_offset: usize },
}

pub struct Decoder {
    options: Options,
    buffer: Vec<Symbol>,
    offset: usize,
    terminal_padding: bool,
    pending_second_padding: Option<usize>,
    wrap_state: WrapState,
    row_symbols: usize,
    closed: bool,
    sticky_error: Option<Error>,
}

impl Decoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate_options(options)?;
        Ok(Self {
            options,
            buffer: Vec::with_capacity(4),
            offset: 0,
            terminal_padding: false,
            pending_second_padding: None,
            wrap_state: WrapState::Row,
            row_symbols: 0,
            closed: false,
            sticky_error: None,
        })
    }

    fn fail(&mut self, error: Error) -> Error {
        self.sticky_error = Some(error.clone());
        error
    }

    fn emit_group(&mut self, group: &[Symbol], out: &mut Vec<u8>) -> Result<(), Error> {
        let mut values = [0u8; 4];
        for (index, symbol) in group.iter().enumerate() {
            if symbol.byte == b'=' {
                continue;
            }
            values[index] = decode_digit(symbol.byte, self.options.alphabet)
                .expect("alphabet was checked while scanning");
        }

        match group.len() {
            4 if group[2].byte == b'=' => {
                out.push((values[0] << 2) | (values[1] >> 4));
                self.terminal_padding = true;
                if self.options.padding == Padding::Forbidden {
                    return Err(Error::BadPadding {
                        offset: group[2].offset,
                    });
                }
            }
            4 if group[3].byte == b'=' => {
                out.push((values[0] << 2) | (values[1] >> 4));
                out.push((values[1] << 4) | (values[2] >> 2));
                self.terminal_padding = true;
                if self.options.padding == Padding::Forbidden {
                    return Err(Error::BadPadding {
                        offset: group[3].offset,
                    });
                }
            }
            4 => {
                out.push((values[0] << 2) | (values[1] >> 4));
                out.push((values[1] << 4) | (values[2] >> 2));
                out.push((values[2] << 6) | values[3]);
            }
            2 => {
                out.push((values[0] << 2) | (values[1] >> 4));
            }
            3 => {
                out.push((values[0] << 2) | (values[1] >> 4));
                out.push((values[1] << 4) | (values[2] >> 2));
            }
            _ => unreachable!(),
        }
        Ok(())
    }

    fn scan_symbol(&mut self, byte: u8, offset: usize, out: &mut Vec<u8>) -> Result<(), Error> {
        if self.terminal_padding {
            return Err(Error::BadPadding { offset });
        }
        if self.pending_second_padding.is_some() && byte != b'=' {
            return Err(Error::BadPadding { offset });
        }

        let position = self.buffer.len();
        if byte == b'=' {
            if self.options.padding == Padding::Forbidden {
                return Err(Error::BadPadding { offset });
            }
            match position {
                0 | 1 => return Err(Error::BadPadding { offset }),
                2 => {
                    self.pending_second_padding = Some(offset);
                    self.buffer.push(Symbol { byte, offset });
                    return Ok(());
                }
                3 => {
                    self.buffer.push(Symbol { byte, offset });
                    let group = self.buffer.clone();
                    self.buffer.clear();
                    self.pending_second_padding = None;
                    return self.emit_group(&group, out);
                }
                _ => unreachable!(),
            }
        }

        if decode_digit(byte, self.options.alphabet).is_none() {
            return Err(Error::InvalidByte { offset, byte });
        }
        self.buffer.push(Symbol { byte, offset });
        if self.buffer.len() == 4 {
            let group = self.buffer.clone();
            self.buffer.clear();
            self.emit_group(&group, out)?;
        }
        Ok(())
    }

    fn scan_wrapped_byte(&mut self, byte: u8, offset: usize, out: &mut Vec<u8>) -> Result<(), Error> {
        let Whitespace::Wrapped { width, ending } = self.options.whitespace else {
            unreachable!()
        };

        match self.wrap_state {
            WrapState::Row => {
                if self.row_symbols == width {
                    self.wrap_state = WrapState::NeedSeparator {
                        first_offset: offset,
                        got_cr: false,
                    };
                    return self.scan_wrapped_byte(byte, offset, out);
                }

                if byte == b'\n' || byte == b'\r' || byte == b' ' || byte == b'\t'
                    || byte == 0x0b || byte == 0x0c
                {
                    return Err(Error::BadWrap { offset });
                }

                self.scan_symbol(byte, offset, out)?;
                self.row_symbols += 1;
                if self.row_symbols == width {
                    self.wrap_state = WrapState::NeedSeparator {
                        first_offset: offset + 1,
                        got_cr: false,
                    };
                }
                Ok(())
            }
            WrapState::NeedSeparator {
                first_offset,
                got_cr,
            } => match ending {
                LineEnding::Lf => {
                    if byte == b'\n' {
                        self.wrap_state = WrapState::NeedSymbol { first_offset };
                        Ok(())
                    } else {
                        Err(Error::BadWrap { offset })
                    }
                }
                LineEnding::CrLf => {
                    if !got_cr {
                        if byte == b'\r' {
                            self.wrap_state = WrapState::NeedSeparator {
                                first_offset,
                                got_cr: true,
                            };
                            Ok(())
                        } else {
                            Err(Error::BadWrap { offset })
                        }
                    } else if byte == b'\n' {
                        self.wrap_state = WrapState::NeedSymbol { first_offset };
                        Ok(())
                    } else {
                        Err(Error::BadWrap { offset })
                    }
                }
            },
            WrapState::NeedSymbol { first_offset } => {
                self.wrap_state = WrapState::Row;
                self.row_symbols = 0;
                self.scan_wrapped_byte(byte, offset, out).map_err(|error| match error {
                    Error::BadWrap { .. } => Error::BadWrap {
                        offset: first_offset,
                    },
                    other => other,
                })
            }
        }
    }

    pub fn push(&mut self, input: &[u8]) -> Result<Vec<u8>, Error> {
        if let Some(error) = &self.sticky_error {
            return Err(error.clone());
        }
        if self.closed {
            return Err(self.fail(Error::AlreadyFinished));
        }

        let mut out = Vec::new();
        for &byte in input {
            let current_offset = self.offset;
            self.offset += 1;

            let result = match self.options.whitespace {
                Whitespace::None => self.scan_symbol(byte, current_offset, &mut out),
                Whitespace::IgnoreAscii => {
                    if matches!(byte, 9 | 10 | 11 | 12 | 13 | 32) {
                        Ok(())
                    } else {
                        self.scan_symbol(byte, current_offset, &mut out)
                    }
                }
                Whitespace::Wrapped { .. } => {
                    self.scan_wrapped_byte(byte, current_offset, &mut out)
                }
            };

            if let Err(error) = result {
                return Err(self.fail(error));
            }
        }
        Ok(out)
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, Error> {
        if let Some(error) = &self.sticky_error {
            return Err(error.clone());
        }
        if self.closed {
            return Err(self.fail(Error::AlreadyFinished));
        }

        if let Some(offset) = self.pending_second_padding {
            return Err(self.fail(Error::BadPadding { offset }));
        }

        if let Whitespace::Wrapped { .. } = self.options.whitespace {
            match self.wrap_state {
                WrapState::NeedSeparator {
                    first_offset,
                    got_cr: false,
                } if self.row_symbols == 0 => {
                    let _ = first_offset;
                }
                WrapState::NeedSeparator {
                    first_offset,
                    got_cr: false,
                } => {
                    if first_offset < self.offset {
                        return Err(self.fail(Error::BadWrap { offset: first_offset }));
                    }
                }
                WrapState::NeedSeparator {
                    first_offset,
                    got_cr: true,
                } => return Err(self.fail(Error::BadWrap { offset: first_offset })),
                WrapState::NeedSymbol { first_offset } => {
                    return Err(self.fail(Error::BadWrap { offset: first_offset }))
                }
                WrapState::Row => {}
            }
        }

        let mut out = Vec::new();
        if !self.buffer.is_empty() {
            if self.options.padding == Padding::Required {
                return Err(self.fail(Error::Truncated {
                    offset: self.buffer[0].offset,
                }));
            }
            if self.buffer.len() == 1 {
                return Err(self.fail(Error::Truncated {
                    offset: self.buffer[0].offset,
                }));
            }
            let group = self.buffer.clone();
            if let Err(error) = self.emit_group(&group, &mut out) {
                return Err(self.fail(error));
            }
            self.buffer.clear();
        }

        self.closed = true;
        Ok(out)
    }
}

pub fn decode(input: &[u8], options: Options) -> Result<Vec<u8>, Error> {
    let mut decoder = Decoder::new(options)?;
    let mut out = decoder.push(input)?;
    out.extend(decoder.finish()?);
    Ok(out)
}

fn main() {}