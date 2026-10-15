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

fn append_wrapped_symbols(
    output: &mut String,
    symbols: &[u8],
    width: Option<usize>,
    ending: LineEnding,
    column: &mut usize,
) {
    for &symbol in symbols {
        if let Some(width) = width {
            if *column == width {
                match ending {
                    LineEnding::Lf => output.push('\n'),
                    LineEnding::CrLf => output.push_str("\r\n"),
                }
                *column = 0;
            }
            *column += 1;
        }
        output.push(symbol as char);
    }
}

fn encode_group(bytes: &[u8], alphabet: Alphabet, padding: Padding) -> Vec<u8> {
    let b0 = bytes[0];
    let b1 = if bytes.len() > 1 { bytes[1] } else { 0 };
    let b2 = if bytes.len() > 2 { bytes[2] } else { 0 };

    let mut out = vec![
        encode_digit(b0 >> 2, alphabet),
        encode_digit(((b0 & 0x03) << 4) | (b1 >> 4), alphabet),
    ];

    if bytes.len() > 1 {
        out.push(encode_digit(((b1 & 0x0f) << 2) | (b2 >> 6), alphabet));
    } else if padding != Padding::Forbidden {
        out.extend_from_slice(b"==");
        return out;
    }

    if bytes.len() > 2 {
        out.push(encode_digit(b2 & 0x3f, alphabet));
    } else if padding != Padding::Forbidden {
        out.push(b'=');
    }

    out
}

pub fn encode(input: &[u8], options: Options) -> Result<String, Error> {
    validate_options(options)?;
    let (width, ending) = match options.whitespace {
        Whitespace::Wrapped { width, ending } => (Some(width), ending),
        _ => (None, LineEnding::Lf),
    };

    let mut output = String::new();
    let mut column = 0usize;
    let mut chunks = input.chunks_exact(3);

    for group in &mut chunks {
        let symbols = encode_group(group, options.alphabet, options.padding);
        append_wrapped_symbols(&mut output, &symbols, width, ending, &mut column);
    }

    let tail = chunks.remainder();
    if !tail.is_empty() {
        let symbols = encode_group(tail, options.alphabet, options.padding);
        append_wrapped_symbols(&mut output, &symbols, width, ending, &mut column);
    }

    Ok(output)
}

pub struct Encoder {
    options: Options,
    tail: Vec<u8>,
    column: usize,
    finished: bool,
}

impl Encoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate_options(options)?;
        Ok(Self {
            options,
            tail: Vec::new(),
            column: 0,
            finished: false,
        })
    }

    pub fn push(&mut self, input: &[u8]) -> Result<String, Error> {
        if self.finished {
            return Err(Error::AlreadyFinished);
        }

        let mut output = String::new();
        let (width, ending) = match self.options.whitespace {
            Whitespace::Wrapped { width, ending } => (Some(width), ending),
            _ => (None, LineEnding::Lf),
        };

        let mut index = 0usize;
        if !self.tail.is_empty() {
            while self.tail.len() < 3 && index < input.len() {
                self.tail.push(input[index]);
                index += 1;
            }
            if self.tail.len() == 3 {
                let symbols = encode_group(&self.tail, self.options.alphabet, self.options.padding);
                append_wrapped_symbols(&mut output, &symbols, width, ending, &mut self.column);
                self.tail.clear();
            }
        }

        while index + 3 <= input.len() {
            let symbols = encode_group(
                &input[index..index + 3],
                self.options.alphabet,
                self.options.padding,
            );
            append_wrapped_symbols(&mut output, &symbols, width, ending, &mut self.column);
            index += 3;
        }

        self.tail.extend_from_slice(&input[index..]);
        Ok(output)
    }

    pub fn finish(&mut self) -> Result<String, Error> {
        if self.finished {
            return Err(Error::AlreadyFinished);
        }
        self.finished = true;

        let mut output = String::new();
        if !self.tail.is_empty() {
            let (width, ending) = match self.options.whitespace {
                Whitespace::Wrapped { width, ending } => (Some(width), ending),
                _ => (None, LineEnding::Lf),
            };
            let symbols = encode_group(&self.tail, self.options.alphabet, self.options.padding);
            append_wrapped_symbols(&mut output, &symbols, width, ending, &mut self.column);
            self.tail.clear();
        }
        Ok(output)
    }
}

#[derive(Clone, Copy)]
struct Symbol {
    value: u8,
    offset: usize,
    byte: u8,
}

enum WrapState {
    Normal,
    NeedLf { cr_offset: usize },
    NeedSymbol { separator_offset: usize },
}

pub struct Decoder {
    options: Options,
    group: Vec<Symbol>,
    offset: usize,
    symbol_count: usize,
    wrapped_column: usize,
    wrap_state: WrapState,
    terminal_padding: bool,
    finished: bool,
    sticky_error: Option<Error>,
}

impl Decoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate_options(options)?;
        Ok(Self {
            options,
            group: Vec::with_capacity(4),
            offset: 0,
            symbol_count: 0,
            wrapped_column: 0,
            wrap_state: WrapState::Normal,
            terminal_padding: false,
            finished: false,
            sticky_error: None,
        })
    }

    fn fail<T>(&mut self, error: Error) -> Result<T, Error> {
        self.sticky_error = Some(error.clone());
        Err(error)
    }

    fn is_ignored_ascii(byte: u8) -> bool {
        matches!(byte, 9 | 10 | 11 | 12 | 13 | 32)
    }

    fn value_for_byte(&self, byte: u8, offset: usize) -> Result<u8, Error> {
        let value = match byte {
            b'A'..=b'Z' => Some(byte - b'A'),
            b'a'..=b'z' => Some(byte - b'a' + 26),
            b'0'..=b'9' => Some(byte - b'0' + 52),
            b'+' if self.options.alphabet == Alphabet::Standard => Some(62),
            b'/' if self.options.alphabet == Alphabet::Standard => Some(63),
            b'-' if self.options.alphabet == Alphabet::UrlSafe => Some(62),
            b'_' if self.options.alphabet == Alphabet::UrlSafe => Some(63),
            _ => None,
        };

        if let Some(value) = value {
            return Ok(value);
        }

        if self.options.strict
            && matches!(
                (self.options.alphabet, byte),
                (Alphabet::Standard, b'-' | b'_') | (Alphabet::UrlSafe, b'+' | b'/')
            )
        {
            return Err(Error::MixedAlphabet { offset });
        }

        Err(Error::InvalidByte { offset, byte })
    }

    fn emit_group(&mut self, output: &mut Vec<u8>) -> Result<(), Error> {
        let group = &self.group;
        let a = group[0].value;
        let b = group[1].value;

        if group[2].byte == b'=' {
            if self.options.strict && (b & 0x0f) != 0 {
                return Err(Error::NonCanonical {
                    offset: group[1].offset,
                });
            }
            output.push((a << 2) | (b >> 4));
            self.terminal_padding = true;
        } else {
            let c = group[2].value;
            if group[3].byte == b'=' {
                if self.options.strict && (c & 0x03) != 0 {
                    return Err(Error::NonCanonical {
                        offset: group[2].offset,
                    });
                }
                output.push((a << 2) | (b >> 4));
                output.push((b << 4) | (c >> 2));
                self.terminal_padding = true;
            } else {
                let d = group[3].value;
                output.push((a << 2) | (b >> 4));
                output.push((b << 4) | (c >> 2));
                output.push((c << 6) | d);
            }
        }

        self.group.clear();
        Ok(())
    }

    fn process_symbol(&mut self, byte: u8, offset: usize, output: &mut Vec<u8>) -> Result<(), Error> {
        if let Some(limit) = self.options.max_symbols {
            if self.symbol_count >= limit {
                return Err(Error::Overlong { offset, limit });
            }
        }
        self.symbol_count += 1;

        if self.terminal_padding {
            return Err(Error::BadPadding { offset });
        }

        if self.group.len() == 2 && self.group[1].byte != b'=' && byte != b'=' {
            // Continue to normal alphabet validation below.
        }

        if self.group.len() == 2 && self.group[1].byte == b'=' {
            return Err(Error::BadPadding { offset });
        }

        if self.group.len() == 3 && self.group[2].byte == b'=' && byte != b'=' {
            return Err(Error::BadPadding { offset });
        }

        if byte == b'=' {
            if self.options.padding == Padding::Forbidden {
                return Err(Error::BadPadding { offset });
            }

            match self.group.len() {
                0 | 1 => return Err(Error::BadPadding { offset }),
                2 => {
                    self.group.push(Symbol {
                        value: 0,
                        offset,
                        byte,
                    });
                    return Ok(());
                }
                3 => {
                    self.group.push(Symbol {
                        value: 0,
                        offset,
                        byte,
                    });
                    self.emit_group(output)?;
                    return Ok(());
                }
                _ => return Err(Error::BadPadding { offset }),
            }
        }

        let value = self.value_for_byte(byte, offset)?;
        self.group.push(Symbol {
            value,
            offset,
            byte,
        });

        if self.group.len() == 4 {
            self.emit_group(output)?;
        }

        Ok(())
    }

    fn process_wrapped_byte(
        &mut self,
        byte: u8,
        offset: usize,
        output: &mut Vec<u8>,
    ) -> Result<(), Error> {
        let (width, ending) = match self.options.whitespace {
            Whitespace::Wrapped { width, ending } => (width, ending),
            _ => unreachable!(),
        };

        match self.wrap_state {
            WrapState::NeedLf { cr_offset } => {
                if byte != b'\n' {
                    return Err(Error::BadWrap { offset });
                }
                self.wrap_state = WrapState::NeedSymbol {
                    separator_offset: cr_offset,
                };
                return Ok(());
            }
            WrapState::NeedSymbol { .. } => {
                if byte == b'\r' || byte == b'\n' || Self::is_ignored_ascii(byte) {
                    return Err(Error::BadWrap { offset });
                }
                self.wrap_state = WrapState::Normal;
                self.wrapped_column = 0;
            }
            WrapState::Normal => {}
        }

        if self.wrapped_column == width {
            match ending {
                LineEnding::Lf if byte == b'\n' => {
                    self.wrap_state = WrapState::NeedSymbol {
                        separator_offset: offset,
                    };
                    return Ok(());
                }
                LineEnding::CrLf if byte == b'\r' => {
                    self.wrap_state = WrapState::NeedLf { cr_offset: offset };
                    return Ok(());
                }
                _ => return Err(Error::BadWrap { offset }),
            }
        }

        if byte == b'\n' || byte == b'\r' || Self::is_ignored_ascii(byte) {
            return Err(Error::BadWrap { offset });
        }

        self.wrapped_column += 1;
        self.process_symbol(byte, offset, output)
    }

    pub fn push(&mut self, input: &[u8]) -> Result<Vec<u8>, Error> {
        if let Some(error) = &self.sticky_error {
            return Err(error.clone());
        }
        if self.finished {
            return self.fail(Error::AlreadyFinished);
        }

        let mut output = Vec::new();
        for &byte in input {
            let offset = self.offset;
            self.offset = self.offset.saturating_add(1);

            let result = match self.options.whitespace {
                Whitespace::None => self.process_symbol(byte, offset, &mut output),
                Whitespace::IgnoreAscii => {
                    if Self::is_ignored_ascii(byte) {
                        Ok(())
                    } else {
                        self.process_symbol(byte, offset, &mut output)
                    }
                }
                Whitespace::Wrapped { .. } => {
                    self.process_wrapped_byte(byte, offset, &mut output)
                }
            };

            if let Err(error) = result {
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
            return self.fail(Error::AlreadyFinished);
        }

        if let WrapState::NeedLf { cr_offset } = self.wrap_state {
            return self.fail(Error::BadWrap { offset: cr_offset });
        }
        if let WrapState::NeedSymbol {
            separator_offset, ..
        } = self.wrap_state
        {
            return self.fail(Error::BadWrap {
                offset: separator_offset,
            });
        }

        if self.group.len() == 3 && self.group[2].byte == b'=' {
            return self.fail(Error::BadPadding {
                offset: self.group[2].offset,
            });
        }

        let mut output = Vec::new();
        match self.group.len() {
            0 => {}
            1 => {
                return self.fail(Error::Truncated {
                    offset: self.group[0].offset,
                });
            }
            2 => {
                if self.options.padding == Padding::Required {
                    return self.fail(Error::Truncated {
                        offset: self.group[0].offset,
                    });
                }
                if self.options.strict && (self.group[1].value & 0x0f) != 0 {
                    return self.fail(Error::NonCanonical {
                        offset: self.group[1].offset,
                    });
                }
                output.push((self.group[0].value << 2) | (self.group[1].value >> 4));
            }
            3 => {
                if self.options.padding == Padding::Required {
                    return self.fail(Error::Truncated {
                        offset: self.group[0].offset,
                    });
                }
                if self.options.strict && (self.group[2].value & 0x03) != 0 {
                    return self.fail(Error::NonCanonical {
                        offset: self.group[2].offset,
                    });
                }
                output.push((self.group[0].value << 2) | (self.group[1].value >> 4));
                output.push((self.group[1].value << 4) | (self.group[2].value >> 2));
            }
            _ => unreachable!(),
        }

        self.group.clear();
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

fn main() {}