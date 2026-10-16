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

fn alphabet_bytes(alphabet: Alphabet) -> &'static [u8; 64] {
    match alphabet {
        Alphabet::Standard => b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/",
        Alphabet::UrlSafe => b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_",
    }
}

fn encode_symbols(input: &[u8], alphabet: Alphabet, padding: Padding) -> Vec<u8> {
    let table = alphabet_bytes(alphabet);
    let mut out = Vec::with_capacity(input.len().div_ceil(3) * 4);

    let mut chunks = input.chunks_exact(3);
    for chunk in &mut chunks {
        let n = ((chunk[0] as u32) << 16) | ((chunk[1] as u32) << 8) | chunk[2] as u32;
        out.push(table[((n >> 18) & 63) as usize]);
        out.push(table[((n >> 12) & 63) as usize]);
        out.push(table[((n >> 6) & 63) as usize]);
        out.push(table[(n & 63) as usize]);
    }

    let rem = chunks.remainder();
    if !rem.is_empty() {
        out.push(table[(rem[0] >> 2) as usize]);
        if rem.len() == 1 {
            out.push(table[((rem[0] & 3) << 4) as usize]);
            if padding != Padding::Forbidden {
                out.extend_from_slice(b"==");
            }
        } else {
            out.push(table[(((rem[0] & 3) << 4) | (rem[1] >> 4)) as usize]);
            out.push(table[((rem[1] & 15) << 2) as usize]);
            if padding != Padding::Forbidden {
                out.push(b'=');
            }
        }
    }

    out
}

fn wrap_symbols(symbols: &[u8], whitespace: Whitespace) -> String {
    match whitespace {
        Whitespace::Wrapped { width, ending } if !symbols.is_empty() => {
            let separator: &[u8] = match ending {
                LineEnding::Lf => b"\n",
                LineEnding::CrLf => b"\r\n",
            };
            let mut out = Vec::with_capacity(symbols.len() + symbols.len() / width * separator.len());
            for (i, chunk) in symbols.chunks(width).enumerate() {
                if i != 0 {
                    out.extend_from_slice(separator);
                }
                out.extend_from_slice(chunk);
            }
            String::from_utf8(out).expect("base64 alphabet is ASCII")
        }
        _ => String::from_utf8(symbols.to_vec()).expect("base64 alphabet is ASCII"),
    }
}

pub fn encode(input: &[u8], options: Options) -> Result<String, Error> {
    validate_options(options)?;
    let symbols = encode_symbols(input, options.alphabet, options.padding);
    Ok(wrap_symbols(&symbols, options.whitespace))
}

pub struct Encoder {
    options: Options,
    tail: Vec<u8>,
    output_symbols: usize,
    finished: bool,
}

impl Encoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate_options(options)?;
        Ok(Self {
            options,
            tail: Vec::new(),
            output_symbols: 0,
            finished: false,
        })
    }

    pub fn push(&mut self, input: &[u8]) -> Result<String, Error> {
        if self.finished {
            return Err(Error::AlreadyFinished);
        }

        let mut bytes = Vec::with_capacity(self.tail.len() + input.len());
        bytes.extend_from_slice(&self.tail);
        bytes.extend_from_slice(input);

        let complete_len = bytes.len() / 3 * 3;
        let symbols = encode_symbols(&bytes[..complete_len], self.options.alphabet, self.options.padding);
        self.tail.clear();
        self.tail.extend_from_slice(&bytes[complete_len..]);

        let output = self.format_new_symbols(&symbols);
        Ok(output)
    }

    pub fn finish(&mut self) -> Result<String, Error> {
        if self.finished {
            return Err(Error::AlreadyFinished);
        }
        self.finished = true;

        let symbols = encode_symbols(&self.tail, self.options.alphabet, self.options.padding);
        self.tail.clear();
        Ok(self.format_new_symbols(&symbols))
    }

    fn format_new_symbols(&mut self, symbols: &[u8]) -> String {
        let output = match self.options.whitespace {
            Whitespace::Wrapped { width, ending } => {
                let separator: &[u8] = match ending {
                    LineEnding::Lf => b"\n",
                    LineEnding::CrLf => b"\r\n",
                };
                let mut bytes = Vec::new();
                for &symbol in symbols {
                    if self.output_symbols > 0 && self.output_symbols % width == 0 {
                        bytes.extend_from_slice(separator);
                    }
                    bytes.push(symbol);
                    self.output_symbols += 1;
                }
                String::from_utf8(bytes).expect("base64 alphabet is ASCII")
            }
            _ => {
                self.output_symbols += symbols.len();
                String::from_utf8(symbols.to_vec()).expect("base64 alphabet is ASCII")
            }
        };
        output
    }
}

#[derive(Clone, Copy)]
struct Symbol {
    value: u8,
    offset: usize,
}

pub struct Decoder {
    options: Options,
    group: Vec<Symbol>,
    terminal_padding: bool,
    pending_second_padding: Option<usize>,
    physical_offset: usize,
    wrapped_symbols: usize,
    wrap_state: WrapState,
    finished: bool,
    sticky_error: Option<Error>,
}

#[derive(Clone, Copy)]
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
            group: Vec::with_capacity(4),
            terminal_padding: false,
            pending_second_padding: None,
            physical_offset: 0,
            wrapped_symbols: 0,
            wrap_state: WrapState::Row,
            finished: false,
            sticky_error: None,
        })
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
            let offset = self.physical_offset;
            self.physical_offset += 1;

            if let Err(error) = self.process_byte(byte, offset, &mut output) {
                self.sticky_error = Some(error.clone());
                return Err(error);
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

        let result = self.finish_inner();
        match result {
            Ok(output) => {
                self.finished = true;
                Ok(output)
            }
            Err(error) => {
                self.sticky_error = Some(error.clone());
                Err(error)
            }
        }
    }

    fn process_byte(
        &mut self,
        byte: u8,
        offset: usize,
        output: &mut Vec<u8>,
    ) -> Result<(), Error> {
        match self.options.whitespace {
            Whitespace::None => self.process_symbol(byte, offset, output),
            Whitespace::IgnoreAscii => {
                if matches!(byte, 9 | 10 | 11 | 12 | 13 | 32) {
                    Ok(())
                } else {
                    self.process_symbol(byte, offset, output)
                }
            }
            Whitespace::Wrapped { width, ending } => {
                match self.wrap_state {
                    WrapState::NeedSeparator => match (ending, byte) {
                        (LineEnding::Lf, b'\n') => {
                            self.wrap_state = WrapState::NeedSymbol {
                                separator_offset: offset,
                            };
                            Ok(())
                        }
                        (LineEnding::CrLf, b'\r') => {
                            self.wrap_state = WrapState::NeedLf { cr_offset: offset };
                            Ok(())
                        }
                        _ => Err(Error::BadWrap { offset }),
                    },
                    WrapState::NeedLf { cr_offset } => {
                        if byte == b'\n' {
                            self.wrap_state = WrapState::NeedSymbol {
                                separator_offset: cr_offset,
                            };
                            Ok(())
                        } else {
                            Err(Error::BadWrap { offset })
                        }
                    }
                    WrapState::NeedSymbol { separator_offset } => {
                        if byte == b'\n' || byte == b'\r' {
                            return Err(Error::BadWrap { offset });
                        }
                        self.wrap_state = WrapState::Row;
                        self.wrapped_symbols = 0;
                        self.process_wrapped_symbol(byte, offset, width, output)
                            .map_err(|_| Error::BadWrap {
                                offset: separator_offset,
                            })
                    }
                    WrapState::Row => {
                        if byte == b'\n' || byte == b'\r' || matches!(byte, 9 | 11 | 12 | 32) {
                            return Err(Error::BadWrap { offset });
                        }
                        self.process_wrapped_symbol(byte, offset, width, output)
                    }
                }
            }
        }
    }

    fn process_wrapped_symbol(
        &mut self,
        byte: u8,
        offset: usize,
        width: usize,
        output: &mut Vec<u8>,
    ) -> Result<(), Error> {
        self.process_symbol(byte, offset, output)?;
        self.wrapped_symbols += 1;
        if self.wrapped_symbols == width {
            self.wrap_state = WrapState::NeedSeparator;
        }
        Ok(())
    }

    fn process_symbol(
        &mut self,
        byte: u8,
        offset: usize,
        output: &mut Vec<u8>,
    ) -> Result<(), Error> {
        if self.terminal_padding {
            return Err(Error::BadPadding { offset });
        }
        if self.pending_second_padding.is_some() && byte != b'=' {
            return Err(Error::BadPadding { offset });
        }

        if byte == b'=' {
            if self.options.padding == Padding::Forbidden {
                return Err(Error::BadPadding { offset });
            }
            match self.group.len() {
                0 | 1 => return Err(Error::BadPadding { offset }),
                2 => {
                    self.pending_second_padding = Some(offset);
                    self.group.push(Symbol { value: 64, offset });
                    return Ok(());
                }
                3 => {
                    self.group.push(Symbol { value: 64, offset });
                    self.complete_group(output)?;
                    self.terminal_padding = true;
                    return Ok(());
                }
                _ => return Err(Error::BadPadding { offset }),
            }
        }

        let value = decode_value(byte, self.options.alphabet)
            .ok_or(Error::InvalidByte { offset, byte })?;
        self.group.push(Symbol { value, offset });
        if self.group.len() == 4 {
            self.complete_group(output)?;
        }
        Ok(())
    }

    fn complete_group(&mut self, output: &mut Vec<u8>) -> Result<(), Error> {
        let a = self.group[0].value;
        let b = self.group[1].value;
        let c = self.group[2].value;
        let d = self.group[3].value;

        if c == 64 {
            if d != 64 {
                return Err(Error::BadPadding {
                    offset: self.group[3].offset,
                });
            }
            output.push((a << 2) | (b >> 4));
            self.terminal_padding = true;
        } else if d == 64 {
            output.push((a << 2) | (b >> 4));
            output.push((b << 4) | (c >> 2));
            self.terminal_padding = true;
        } else {
            output.push((a << 2) | (b >> 4));
            output.push((b << 4) | (c >> 2));
            output.push((c << 6) | d);
        }

        self.group.clear();
        self.pending_second_padding = None;
        Ok(())
    }

    fn finish_inner(&mut self) -> Result<Vec<u8>, Error> {
        if let Whitespace::Wrapped { .. } = self.options.whitespace {
            match self.wrap_state {
                WrapState::NeedLf { cr_offset } => {
                    return Err(Error::BadWrap { offset: cr_offset });
                }
                WrapState::NeedSymbol { separator_offset } => {
                    return Err(Error::BadWrap {
                        offset: separator_offset,
                    });
                }
                _ => {}
            }

            if matches!(self.wrap_state, WrapState::NeedSeparator) {
                self.wrap_state = WrapState::Row;
            }
        }

        if let Some(offset) = self.pending_second_padding {
            return Err(Error::BadPadding { offset });
        }

        if self.group.is_empty() {
            return Ok(Vec::new());
        }

        match self.options.padding {
            Padding::Required => Err(Error::Truncated {
                offset: self.group[0].offset,
            }),
            Padding::Forbidden | Padding::Optional => match self.group.len() {
                1 => Err(Error::Truncated {
                    offset: self.group[0].offset,
                }),
                2 => Ok(vec![
                    (self.group[0].value << 2) | (self.group[1].value >> 4),
                ]),
                3 => Ok(vec![
                    (self.group[0].value << 2) | (self.group[1].value >> 4),
                    (self.group[1].value << 4) | (self.group[2].value >> 2),
                ]),
                _ => unreachable!(),
            },
        }
    }
}

fn decode_value(byte: u8, alphabet: Alphabet) -> Option<u8> {
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

pub fn decode(input: &[u8], options: Options) -> Result<Vec<u8>, Error> {
    validate_options(options)?;
    let mut decoder = Decoder::new(options)?;
    let mut output = decoder.push(input)?;
    output.extend(decoder.finish()?);
    Ok(output)
}