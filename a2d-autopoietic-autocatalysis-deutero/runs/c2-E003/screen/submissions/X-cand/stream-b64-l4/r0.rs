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
    pub strict: bool,
    pub max_symbols: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    InvalidByte { offset: usize, byte: u8 },
    BadPadding { offset: usize },
    Truncated { offset: usize },
    AlreadyFinished,
    InvalidWidth { width: usize },
    BadWrap { offset: usize },
    NonCanonical { offset: usize },
    MixedAlphabet { offset: usize },
    Overlong { offset: usize, limit: usize },
}

fn validate(options: Options) -> Result<(), Error> {
    if let Whitespace::Wrapped { width, .. } = options.whitespace {
        if !(4..=120).contains(&width) || width % 4 != 0 {
            return Err(Error::InvalidWidth { width });
        }
    }
    Ok(())
}

fn alphabet_byte(alphabet: Alphabet, value: u8) -> u8 {
    match value {
        0..=25 => b'A' + value,
        26..=51 => b'a' + value - 26,
        52..=61 => b'0' + value - 52,
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

fn digit_value(byte: u8, alphabet: Alphabet) -> Option<u8> {
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

fn is_foreign_alphabet_byte(byte: u8, alphabet: Alphabet) -> bool {
    match alphabet {
        Alphabet::Standard => byte == b'-' || byte == b'_',
        Alphabet::UrlSafe => byte == b'+' || byte == b'/',
    }
}

fn is_ignored_ascii(byte: u8) -> bool {
    matches!(byte, 9 | 10 | 11 | 12 | 13 | 32)
}

fn push_symbol(
    output: &mut String,
    options: Options,
    row_symbols: &mut usize,
    symbol: u8,
) {
    if let Whitespace::Wrapped { width, ending } = options.whitespace {
        if *row_symbols == width {
            match ending {
                LineEnding::Lf => output.push('\n'),
                LineEnding::CrLf => output.push_str("\r\n"),
            }
            *row_symbols = 0;
        }
    }
    output.push(symbol as char);
    *row_symbols += 1;
}

fn encode_symbols(input: &[u8], options: Options, row_symbols: &mut usize) -> String {
    let mut out = String::new();
    let mut chunks = input.chunks_exact(3);
    for chunk in &mut chunks {
        let a = chunk[0];
        let b = chunk[1];
        let c = chunk[2];
        let digits = [
            a >> 2,
            ((a & 0x03) << 4) | (b >> 4),
            ((b & 0x0f) << 2) | (c >> 6),
            c & 0x3f,
        ];
        for digit in digits {
            push_symbol(&mut out, options, row_symbols, alphabet_byte(options.alphabet, digit));
        }
    }
    let tail = chunks.remainder();
    if !tail.is_empty() {
        let a = tail[0];
        let second = if tail.len() == 2 { tail[1] } else { 0 };
        let digits = [
            a >> 2,
            ((a & 0x03) << 4) | (second >> 4),
            (second & 0x0f) << 2,
        ];
        push_symbol(&mut out, options, row_symbols, alphabet_byte(options.alphabet, digits[0]));
        push_symbol(&mut out, options, row_symbols, alphabet_byte(options.alphabet, digits[1]));
        if tail.len() == 2 {
            push_symbol(&mut out, options, row_symbols, alphabet_byte(options.alphabet, digits[2]));
            if options.padding != Padding::Forbidden {
                push_symbol(&mut out, options, row_symbols, b'=');
            }
        } else if options.padding != Padding::Forbidden {
            push_symbol(&mut out, options, row_symbols, b'=');
            push_symbol(&mut out, options, row_symbols, b'=');
        }
    }
    out
}

pub fn encode(input: &[u8], options: Options) -> Result<String, Error> {
    validate(options)?;
    let mut row_symbols = 0;
    Ok(encode_symbols(input, options, &mut row_symbols))
}

pub struct Encoder {
    options: Options,
    buffered: Vec<u8>,
    row_symbols: usize,
    closed: bool,
}

impl Encoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate(options)?;
        Ok(Self {
            options,
            buffered: Vec::new(),
            row_symbols: 0,
            closed: false,
        })
    }

    pub fn push(&mut self, input: &[u8]) -> Result<String, Error> {
        if self.closed {
            return Err(Error::AlreadyFinished);
        }
        if input.is_empty() {
            return Ok(String::new());
        }
        let mut bytes = Vec::with_capacity(self.buffered.len() + input.len());
        bytes.extend_from_slice(&self.buffered);
        bytes.extend_from_slice(input);
        let complete_len = bytes.len() / 3 * 3;
        let out = encode_symbols(&bytes[..complete_len], self.options, &mut self.row_symbols);
        self.buffered.clear();
        self.buffered.extend_from_slice(&bytes[complete_len..]);
        Ok(out)
    }

    pub fn finish(&mut self) -> Result<String, Error> {
        if self.closed {
            return Err(Error::AlreadyFinished);
        }
        self.closed = true;
        let out = encode_symbols(&self.buffered, self.options, &mut self.row_symbols);
        self.buffered.clear();
        Ok(out)
    }
}

#[derive(Clone, Copy)]
enum WrapState {
    Normal,
    NeedLf { cr_offset: usize },
    NeedSymbol { separator_offset: usize },
}

pub struct Decoder {
    options: Options,
    offset: usize,
    symbols_seen: usize,
    group: [u8; 4],
    group_offsets: [usize; 4],
    group_len: usize,
    pending_second_pad: Option<usize>,
    padded_terminal: bool,
    row_symbols: usize,
    wrap_state: WrapState,
    output: Vec<u8>,
    closed: bool,
    sticky: Option<Error>,
}

impl Decoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate(options)?;
        Ok(Self {
            options,
            offset: 0,
            symbols_seen: 0,
            group: [0; 4],
            group_offsets: [0; 4],
            group_len: 0,
            pending_second_pad: None,
            padded_terminal: false,
            row_symbols: 0,
            wrap_state: WrapState::Normal,
            output: Vec::new(),
            closed: false,
            sticky: None,
        })
    }

    fn fail<T>(&mut self, error: Error) -> Result<T, Error> {
        self.sticky = Some(error.clone());
        Err(error)
    }

    fn emit_group(&mut self) -> Result<(), Error> {
        let a = self.group[0];
        let b = self.group[1];
        let c = self.group[2];
        let d = self.group[3];

        if c == 64 {
            if self.options.strict && (b & 0x0f) != 0 {
                return self.fail(Error::NonCanonical {
                    offset: self.group_offsets[1],
                });
            }
            self.output.push((a << 2) | (b >> 4));
            self.padded_terminal = true;
        } else if d == 64 {
            if self.options.strict && (c & 0x03) != 0 {
                return self.fail(Error::NonCanonical {
                    offset: self.group_offsets[2],
                });
            }
            self.output.push((a << 2) | (b >> 4));
            self.output.push((b << 4) | (c >> 2));
            self.padded_terminal = true;
        } else {
            self.output.push((a << 2) | (b >> 4));
            self.output.push((b << 4) | (c >> 2));
            self.output.push((c << 6) | d);
        }
        self.group_len = 0;
        self.pending_second_pad = None;
        Ok(())
    }

    fn consume_symbol(&mut self, byte: u8, offset: usize) -> Result<(), Error> {
        if self.options.max_symbols.is_some_and(|limit| self.symbols_seen >= limit) {
            return self.fail(Error::Overlong {
                offset,
                limit: self.options.max_symbols.unwrap(),
            });
        }
        self.symbols_seen += 1;

        if self.padded_terminal {
            return self.fail(Error::BadPadding { offset });
        }
        if self.pending_second_pad.is_some() {
            if byte != b'=' {
                return self.fail(Error::BadPadding { offset });
            }
            self.group[3] = 64;
            self.group_offsets[3] = offset;
            self.group_len = 4;
            return self.emit_group();
        }
        if byte == b'=' {
            match self.group_len {
                0 | 1 => return self.fail(Error::BadPadding { offset }),
                2 => {
                    self.pending_second_pad = Some(offset);
                    self.group[2] = 64;
                    self.group_offsets[2] = offset;
                    self.group_len = 3;
                    return Ok(());
                }
                3 => {
                    if self.options.padding == Padding::Forbidden {
                        return self.fail(Error::BadPadding { offset });
                    }
                    self.group[3] = 64;
                    self.group_offsets[3] = offset;
                    self.group_len = 4;
                    return self.emit_group();
                }
                _ => return self.fail(Error::BadPadding { offset }),
            }
        }
        if self.options.padding == Padding::Forbidden && byte == b'=' {
            return self.fail(Error::BadPadding { offset });
        }
        let Some(value) = digit_value(byte, self.options.alphabet) else {
            if self.options.strict && is_foreign_alphabet_byte(byte, self.options.alphabet) {
                return self.fail(Error::MixedAlphabet { offset });
            }
            return self.fail(Error::InvalidByte { offset, byte });
        };
        let position = self.group_len;
        self.group[position] = value;
        self.group_offsets[position] = offset;
        self.group_len += 1;
        if self.group_len == 4 {
            self.emit_group()?;
        }
        Ok(())
    }

    fn consume_wrapped_byte(&mut self, byte: u8, offset: usize) -> Result<(), Error> {
        let (width, ending) = match self.options.whitespace {
            Whitespace::Wrapped { width, ending } => (width, ending),
            _ => unreachable!(),
        };
        match self.wrap_state {
            WrapState::NeedLf { cr_offset } => {
                if byte != b'\n' {
                    return self.fail(Error::BadWrap { offset });
                }
                self.wrap_state = WrapState::NeedSymbol {
                    separator_offset: cr_offset,
                };
                Ok(())
            }
            WrapState::NeedSymbol { .. } => {
                if byte == b'\r' || byte == b'\n' {
                    return self.fail(Error::BadWrap { offset });
                }
                self.wrap_state = WrapState::Normal;
                self.row_symbols = 0;
                self.consume_symbol(byte, offset)
            }
            WrapState::Normal => {
                if self.row_symbols == width {
                    match ending {
                        LineEnding::Lf if byte == b'\n' => {
                            self.wrap_state = WrapState::NeedSymbol {
                                separator_offset: offset,
                            };
                            Ok(())
                        }
                        LineEnding::Lf => self.fail(Error::BadWrap { offset }),
                        LineEnding::CrLf if byte == b'\r' => {
                            self.wrap_state = WrapState::NeedLf { cr_offset: offset };
                            Ok(())
                        }
                        LineEnding::CrLf => self.fail(Error::BadWrap { offset }),
                    }
                } else if byte == b'\r' || byte == b'\n' {
                    self.fail(Error::BadWrap { offset })
                } else {
                    self.row_symbols += 1;
                    self.consume_symbol(byte, offset)
                }
            }
        }
    }

    fn process_byte(&mut self, byte: u8, offset: usize) -> Result<(), Error> {
        match self.options.whitespace {
            Whitespace::None => self.consume_symbol(byte, offset),
            Whitespace::IgnoreAscii => {
                if is_ignored_ascii(byte) {
                    Ok(())
                } else {
                    self.consume_symbol(byte, offset)
                }
            }
            Whitespace::Wrapped { .. } => self.consume_wrapped_byte(byte, offset),
        }
    }

    pub fn push(&mut self, input: &[u8]) -> Result<Vec<u8>, Error> {
        if let Some(error) = &self.sticky {
            return Err(error.clone());
        }
        if self.closed {
            return Err(Error::AlreadyFinished);
        }
        let start = self.output.len();
        for &byte in input {
            let offset = self.offset;
            self.offset += 1;
            if let Err(error) = self.process_byte(byte, offset) {
                return Err(error);
            }
        }
        Ok(self.output.drain(start..).collect())
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, Error> {
        if let Some(error) = &self.sticky {
            return Err(error.clone());
        }
        if self.closed {
            return Err(Error::AlreadyFinished);
        }

        match self.wrap_state {
            WrapState::NeedLf { cr_offset } => {
                return self.fail(Error::BadWrap { offset: cr_offset });
            }
            WrapState::NeedSymbol { separator_offset } => {
                return self.fail(Error::BadWrap {
                    offset: separator_offset,
                });
            }
            WrapState::Normal => {}
        }

        if let Some(offset) = self.pending_second_pad {
            return self.fail(Error::BadPadding { offset });
        }

        if self.group_len == 1 {
            return self.fail(Error::Truncated {
                offset: self.group_offsets[0],
            });
        }

        if self.group_len > 0 {
            if self.options.padding == Padding::Required {
                return self.fail(Error::Truncated {
                    offset: self.group_offsets[0],
                });
            }
            if self.group_len == 2 {
                if self.options.strict && (self.group[1] & 0x0f) != 0 {
                    return self.fail(Error::NonCanonical {
                        offset: self.group_offsets[1],
                    });
                }
                self.output.push((self.group[0] << 2) | (self.group[1] >> 4));
            } else if self.group_len == 3 {
                if self.options.strict && (self.group[2] & 0x03) != 0 {
                    return self.fail(Error::NonCanonical {
                        offset: self.group_offsets[2],
                    });
                }
                self.output.push((self.group[0] << 2) | (self.group[1] >> 4));
                self.output
                    .push((self.group[1] << 4) | (self.group[2] >> 2));
            }
            self.group_len = 0;
        }

        self.closed = true;
        Ok(std::mem::take(&mut self.output))
    }
}

pub fn decode(input: &[u8], options: Options) -> Result<Vec<u8>, Error> {
    validate(options)?;
    let mut decoder = Decoder::new(options)?;
    let mut output = decoder.push(input)?;
    output.extend(decoder.finish()?);
    Ok(output)
}