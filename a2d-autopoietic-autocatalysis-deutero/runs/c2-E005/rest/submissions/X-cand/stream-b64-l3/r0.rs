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

fn alphabet_digit(byte: u8, alphabet: Alphabet) -> Option<u8> {
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

fn encoded_digit(value: u8, alphabet: Alphabet) -> u8 {
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

fn encode_raw(input: &[u8], options: Options, allow_padding: bool) -> String {
    let mut output = Vec::with_capacity((input.len() + 2) / 3 * 4);
    let mut chunks = input.chunks_exact(3);

    for chunk in &mut chunks {
        let a = chunk[0];
        let b = chunk[1];
        let c = chunk[2];
        output.push(encoded_digit(a >> 2, options.alphabet));
        output.push(encoded_digit(((a & 0x03) << 4) | (b >> 4), options.alphabet));
        output.push(encoded_digit(((b & 0x0f) << 2) | (c >> 6), options.alphabet));
        output.push(encoded_digit(c & 0x3f, options.alphabet));
    }

    let tail = chunks.remainder();
    if !tail.is_empty() {
        let a = tail[0];
        output.push(encoded_digit(a >> 2, options.alphabet));
        if tail.len() == 1 {
            output.push(encoded_digit((a & 0x03) << 4, options.alphabet));
            if allow_padding {
                output.extend_from_slice(b"==");
            }
        } else {
            let b = tail[1];
            output.push(encoded_digit(((a & 0x03) << 4) | (b >> 4), options.alphabet));
            output.push(encoded_digit((b & 0x0f) << 2, options.alphabet));
            if allow_padding {
                output.push(b'=');
            }
        }
    }

    String::from_utf8(output).expect("base64 alphabet is ASCII")
}

fn wrap_encoded(raw: String, whitespace: Whitespace) -> String {
    let Whitespace::Wrapped { width, ending } = whitespace else {
        return raw;
    };
    if raw.len() <= width {
        return raw;
    }

    let separator = match ending {
        LineEnding::Lf => "\n",
        LineEnding::CrLf => "\r\n",
    };

    let mut result = String::with_capacity(raw.len() + raw.len() / width * separator.len());
    for (index, chunk) in raw.as_bytes().chunks(width).enumerate() {
        if index != 0 {
            result.push_str(separator);
        }
        result.push_str(std::str::from_utf8(chunk).expect("ASCII output"));
    }
    result
}

pub fn encode(input: &[u8], options: Options) -> Result<String, Error> {
    validate_options(options)?;
    let allow_padding = options.padding != Padding::Forbidden;
    let raw = encode_raw(input, options, allow_padding);
    Ok(wrap_encoded(raw, options.whitespace))
}

pub struct Encoder {
    options: Options,
    buffered: Vec<u8>,
    finished: bool,
}

impl Encoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate_options(options)?;
        Ok(Self {
            options,
            buffered: Vec::with_capacity(2),
            finished: false,
        })
    }

    pub fn push(&mut self, input: &[u8]) -> Result<String, Error> {
        if self.finished {
            return Err(Error::AlreadyFinished);
        }
        if input.is_empty() {
            return Ok(String::new());
        }

        let mut combined = Vec::with_capacity(self.buffered.len() + input.len());
        combined.extend_from_slice(&self.buffered);
        combined.extend_from_slice(input);

        let complete_len = combined.len() / 3 * 3;
        self.buffered.clear();
        self.buffered.extend_from_slice(&combined[complete_len..]);

        let raw = encode_raw(&combined[..complete_len], self.options, true);
        Ok(wrap_encoded(raw, self.options.whitespace))
    }

    pub fn finish(&mut self) -> Result<String, Error> {
        if self.finished {
            return Err(Error::AlreadyFinished);
        }
        self.finished = true;
        let raw = encode_raw(&self.buffered, self.options, true);
        self.buffered.clear();
        Ok(wrap_encoded(raw, self.options.whitespace))
    }
}

#[derive(Clone, Copy)]
struct Symbol {
    byte: u8,
    offset: usize,
}

pub struct Decoder {
    options: Options,
    offset: usize,
    symbols: Vec<Symbol>,
    pending_second_padding: Option<usize>,
    terminal_padding: bool,
    wrapped_symbols: usize,
    wrap_state: WrapState,
    finished: bool,
    sticky_error: Option<Error>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum WrapState {
    Row,
    NeedSeparator,
    NeedLf { cr_offset: usize },
    NeedSymbol { separator_offset: usize },
}

impl Decoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate_options(options)?;
        Ok(Self {
            options,
            offset: 0,
            symbols: Vec::with_capacity(4),
            pending_second_padding: None,
            terminal_padding: false,
            wrapped_symbols: 0,
            wrap_state: WrapState::Row,
            finished: false,
            sticky_error: None,
        })
    }

    fn fail(&mut self, error: Error) -> Result<Vec<u8>, Error> {
        self.sticky_error = Some(error.clone());
        Err(error)
    }

    fn scan_symbol(&mut self, byte: u8, offset: usize, output: &mut Vec<u8>) -> Result<(), Error> {
        let position = self.symbols.len();

        if self.terminal_padding {
            return Err(Error::BadPadding { offset });
        }

        if let Some(pending_offset) = self.pending_second_padding {
            if byte != b'=' {
                return Err(Error::BadPadding { offset });
            }
            if self.options.padding == Padding::Forbidden {
                return Err(Error::BadPadding { offset });
            }
            self.pending_second_padding = None;
            self.symbols.push(Symbol { byte, offset });
            self.emit_group(output);
            self.terminal_padding = true;
            return Ok(());
        }

        if byte == b'=' {
            match position {
                0 | 1 => return Err(Error::BadPadding { offset }),
                2 => {
                    if self.options.padding == Padding::Forbidden {
                        return Err(Error::BadPadding { offset });
                    }
                    self.pending_second_padding = Some(offset);
                    self.symbols.push(Symbol { byte, offset });
                    return Ok(());
                }
                3 => {
                    if self.options.padding == Padding::Forbidden {
                        return Err(Error::BadPadding { offset });
                    }
                    if self.symbols[2].byte == b'=' {
                        return Err(Error::BadPadding { offset });
                    }
                    self.symbols.push(Symbol { byte, offset });
                    self.emit_group(output);
                    self.terminal_padding = true;
                    return Ok(());
                }
                _ => unreachable!(),
            }
        }

        if alphabet_digit(byte, self.options.alphabet).is_none() {
            return Err(Error::InvalidByte { offset, byte });
        }

        self.symbols.push(Symbol { byte, offset });
        if self.symbols.len() == 4 {
            self.emit_group(output);
        }
        Ok(())
    }

    fn emit_group(&mut self, output: &mut Vec<u8>) {
        let digits: Vec<Option<u8>> = self
            .symbols
            .iter()
            .map(|s| {
                if s.byte == b'=' {
                    None
                } else {
                    alphabet_digit(s.byte, self.options.alphabet)
                }
            })
            .collect();

        let a = digits[0].unwrap();
        let b = digits[1].unwrap();
        output.push((a << 2) | (b >> 4));

        if digits[2].is_some() {
            let c = digits[2].unwrap();
            output.push((b << 4) | (c >> 2));
            if digits[3].is_some() {
                output.push((c << 6) | digits[3].unwrap());
            }
        }

        self.symbols.clear();
    }

    fn frame_byte(&mut self, byte: u8, offset: usize, output: &mut Vec<u8>) -> Result<(), Error> {
        match self.options.whitespace {
            Whitespace::None => self.scan_symbol(byte, offset, output),
            Whitespace::IgnoreAscii => {
                if matches!(byte, 9 | 10 | 11 | 12 | 13 | 32) {
                    Ok(())
                } else {
                    self.scan_symbol(byte, offset, output)
                }
            }
            Whitespace::Wrapped { width, ending } => {
                match self.wrap_state {
                    WrapState::Row => {
                        if byte == b'\n' || byte == b'\r' || matches!(byte, 9 | 11 | 12 | 32) {
                            return Err(Error::BadWrap { offset });
                        }
                        self.scan_symbol(byte, offset, output)?;
                        self.wrapped_symbols += 1;
                        if self.wrapped_symbols == width {
                            self.wrap_state = WrapState::NeedSeparator;
                        }
                        Ok(())
                    }
                    WrapState::NeedSeparator => match ending {
                        LineEnding::Lf if byte == b'\n' => {
                            self.wrapped_symbols = 0;
                            self.wrap_state = WrapState::NeedSymbol {
                                separator_offset: offset,
                            };
                            Ok(())
                        }
                        LineEnding::CrLf if byte == b'\r' => {
                            self.wrap_state = WrapState::NeedLf { cr_offset: offset };
                            Ok(())
                        }
                        _ => Err(Error::BadWrap { offset }),
                    },
                    WrapState::NeedLf { .. } if byte == b'\n' => {
                        self.wrapped_symbols = 0;
                        let cr_offset = match self.wrap_state {
                            WrapState::NeedLf { cr_offset } => cr_offset,
                            _ => unreachable!(),
                        };
                        self.wrap_state = WrapState::NeedSymbol {
                            separator_offset: cr_offset,
                        };
                        Ok(())
                    }
                    WrapState::NeedLf { .. } => Err(Error::BadWrap { offset }),
                    WrapState::NeedSymbol { .. } => {
                        if byte == b'\n' || byte == b'\r' || matches!(byte, 9 | 11 | 12 | 32) {
                            return Err(Error::BadWrap { offset });
                        }
                        self.wrap_state = WrapState::Row;
                        self.scan_symbol(byte, offset, output)?;
                        self.wrapped_symbols = 1;
                        if self.wrapped_symbols == width {
                            self.wrap_state = WrapState::NeedSeparator;
                        }
                        Ok(())
                    }
                }
            }
        }
    }

    pub fn push(&mut self, input: &[u8]) -> Result<Vec<u8>, Error> {
        if let Some(error) = &self.sticky_error {
            return Err(error.clone());
        }
        if self.finished {
            return Err(Error::AlreadyFinished);
        }

        let mut output = Vec::new();
        for &byte in input {
            let offset = self.offset;
            self.offset += 1;
            if let Err(error) = self.frame_byte(byte, offset, &mut output) {
                return self.fail(error);
            }
        }
        Ok(output)
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, Error> {
        if let Some(error) = &self.sticky_error {
            return Err(error.clone());
        }
        if self.finished {
            return Err(Error::AlreadyFinished);
        }

        if let Whitespace::Wrapped { ending, .. } = self.options.whitespace {
            match self.wrap_state {
                WrapState::NeedLf { cr_offset } => {
                    return self.fail(Error::BadWrap { offset: cr_offset });
                }
                WrapState::NeedSymbol { separator_offset } => {
                    return self.fail(Error::BadWrap {
                        offset: separator_offset,
                    });
                }
                _ => {}
            }
            let _ = ending;
        }

        if let Some(offset) = self.pending_second_padding {
            return self.fail(Error::BadPadding { offset });
        }

        let mut output = Vec::new();
        let residual = self.symbols.len();
        if residual != 0 {
            if self.options.padding == Padding::Required {
                return self.fail(Error::Truncated {
                    offset: self.symbols[0].offset,
                });
            }
            match residual {
                1 => {
                    return self.fail(Error::Truncated {
                        offset: self.symbols[0].offset,
                    });
                }
                2 => {
                    let a = alphabet_digit(self.symbols[0].byte, self.options.alphabet).unwrap();
                    let b = alphabet_digit(self.symbols[1].byte, self.options.alphabet).unwrap();
                    output.push((a << 2) | (b >> 4));
                }
                3 => {
                    let a = alphabet_digit(self.symbols[0].byte, self.options.alphabet).unwrap();
                    let b = alphabet_digit(self.symbols[1].byte, self.options.alphabet).unwrap();
                    let c = alphabet_digit(self.symbols[2].byte, self.options.alphabet).unwrap();
                    output.push((a << 2) | (b >> 4));
                    output.push((b << 4) | (c >> 2));
                }
                _ => unreachable!(),
            }
            self.symbols.clear();
        }

        self.finished = true;
        Ok(output)
    }
}

pub fn decode(input: &[u8], options: Options) -> Result<Vec<u8>, Error> {
    validate_options(options)?;
    let mut decoder = Decoder::new(options)?;
    let mut output = decoder.push(input)?;
    output.extend(decoder.finish()?);
    Ok(output)
}