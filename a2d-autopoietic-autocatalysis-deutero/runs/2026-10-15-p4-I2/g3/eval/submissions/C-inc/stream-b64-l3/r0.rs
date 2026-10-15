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

fn validate(options: Options) -> Result<(), Error> {
    if let Whitespace::Wrapped { width, .. } = options.whitespace {
        if !(4..=120).contains(&width) || width % 4 != 0 {
            return Err(Error::InvalidWidth { width });
        }
    }
    Ok(())
}

fn alphabet_byte(value: u8, alphabet: Alphabet) -> u8 {
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

fn encode_group(bytes: &[u8], alphabet: Alphabet, padding: Padding) -> String {
    let a = bytes[0];
    let b = if bytes.len() > 1 { bytes[1] } else { 0 };
    let c = if bytes.len() > 2 { bytes[2] } else { 0 };

    let mut out = String::with_capacity(4);
    out.push(alphabet_byte(a >> 2, alphabet) as char);
    out.push(alphabet_byte(((a & 0x03) << 4) | (b >> 4), alphabet) as char);

    if bytes.len() > 1 {
        out.push(alphabet_byte(((b & 0x0f) << 2) | (c >> 6), alphabet) as char);
    } else if padding != Padding::Forbidden {
        out.push('=');
    }

    if bytes.len() > 2 {
        out.push(alphabet_byte(c & 0x3f, alphabet) as char);
    } else if padding != Padding::Forbidden {
        out.push('=');
    }

    out
}

fn wrap_symbols(symbols: &str, whitespace: Whitespace, symbols_in_row: &mut usize) -> String {
    let Whitespace::Wrapped { width, ending } = whitespace else {
        return symbols.to_owned();
    };

    let separator = match ending {
        LineEnding::Lf => "\n",
        LineEnding::CrLf => "\r\n",
    };

    let mut out = String::new();
    for byte in symbols.bytes() {
        if *symbols_in_row == width {
            out.push_str(separator);
            *symbols_in_row = 0;
        }
        out.push(byte as char);
        *symbols_in_row += 1;
    }
    out
}

pub fn encode(input: &[u8], options: Options) -> Result<String, Error> {
    validate(options)?;
    let mut encoder = Encoder::new(options)?;
    let mut out = encoder.push(input)?;
    out.push_str(&encoder.finish()?);
    Ok(out)
}

pub fn decode(input: &[u8], options: Options) -> Result<Vec<u8>, Error> {
    validate(options)?;
    let mut decoder = Decoder::new(options)?;
    let mut out = decoder.push(input)?;
    out.extend(decoder.finish()?);
    Ok(out)
}

pub struct Encoder {
    options: Options,
    tail: Vec<u8>,
    symbols_in_row: usize,
    finished: bool,
}

impl Encoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate(options)?;
        Ok(Self {
            options,
            tail: Vec::new(),
            symbols_in_row: 0,
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
        let mut symbols = String::with_capacity(complete_len / 3 * 4);
        for group in bytes[..complete_len].chunks_exact(3) {
            symbols.push_str(&encode_group(
                group,
                self.options.alphabet,
                self.options.padding,
            ));
        }

        self.tail.clear();
        self.tail.extend_from_slice(&bytes[complete_len..]);
        Ok(wrap_symbols(
            &symbols,
            self.options.whitespace,
            &mut self.symbols_in_row,
        ))
    }

    pub fn finish(&mut self) -> Result<String, Error> {
        if self.finished {
            return Err(Error::AlreadyFinished);
        }
        self.finished = true;

        let symbols = if self.tail.is_empty() {
            String::new()
        } else {
            encode_group(
                &self.tail,
                self.options.alphabet,
                self.options.padding,
            )
        };
        self.tail.clear();

        Ok(wrap_symbols(
            &symbols,
            self.options.whitespace,
            &mut self.symbols_in_row,
        ))
    }
}

#[derive(Clone, Copy)]
enum FrameState {
    Row,
    NeedLf { cr_offset: usize, sep_offset: usize },
    NeedSymbol { sep_offset: usize },
}

pub struct Decoder {
    options: Options,
    group: Vec<(u8, usize)>,
    total_offset: usize,
    symbols_in_row: usize,
    frame: FrameState,
    terminal_padding: bool,
    sticky_error: Option<Error>,
    finished: bool,
}

impl Decoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate(options)?;
        Ok(Self {
            options,
            group: Vec::with_capacity(4),
            total_offset: 0,
            symbols_in_row: 0,
            frame: FrameState::Row,
            terminal_padding: false,
            sticky_error: None,
            finished: false,
        })
    }

    fn scan_symbol(
        &mut self,
        byte: u8,
        offset: usize,
        out: &mut Vec<u8>,
    ) -> Result<(), Error> {
        if self.terminal_padding {
            return Err(Error::BadPadding { offset });
        }

        let pos = self.group.len();

        if pos == 3 && self.group[2].0 == b'=' && byte != b'=' {
            return Err(Error::BadPadding { offset });
        }

        if (pos == 0 || pos == 1) && byte == b'=' {
            return Err(Error::BadPadding { offset });
        }

        if byte != b'=' && digit_value(byte, self.options.alphabet).is_none() {
            return Err(Error::InvalidByte { offset, byte });
        }

        if byte == b'=' && self.options.padding == Padding::Forbidden {
            return Err(Error::BadPadding { offset });
        }

        self.group.push((byte, offset));

        if self.group.len() == 4 {
            let a = digit_value(self.group[0].0, self.options.alphabet).unwrap();
            let b = digit_value(self.group[1].0, self.options.alphabet).unwrap();
            let third = self.group[2].0;
            let fourth = self.group[3].0;

            if third == b'=' {
                out.push((a << 2) | (b >> 4));
                self.terminal_padding = true;
            } else {
                let c = digit_value(third, self.options.alphabet).unwrap();
                out.push((a << 2) | (b >> 4));
                out.push((b << 4) | (c >> 2));
                if fourth == b'=' {
                    self.terminal_padding = true;
                } else {
                    let d = digit_value(fourth, self.options.alphabet).unwrap();
                    out.push((c << 6) | d);
                }
            }
            self.group.clear();
        }
        Ok(())
    }

    fn fail<T>(&mut self, error: Error) -> Result<T, Error> {
        self.sticky_error = Some(error.clone());
        Err(error)
    }

    pub fn push(&mut self, input: &[u8]) -> Result<Vec<u8>, Error> {
        if let Some(error) = &self.sticky_error {
            return Err(error.clone());
        }
        if self.finished {
            return self.fail(Error::AlreadyFinished);
        }

        let mut out = Vec::new();

        for &byte in input {
            let offset = self.total_offset;
            self.total_offset += 1;

            if matches!(self.options.whitespace, Whitespace::IgnoreAscii)
                && matches!(byte, 9 | 10 | 11 | 12 | 13 | 32)
            {
                continue;
            }

            if let Whitespace::Wrapped { width, ending } = self.options.whitespace {
                match self.frame {
                    FrameState::NeedLf { sep_offset, .. } => {
                        if byte != b'\n' {
                            return self.fail(Error::BadWrap { offset });
                        }
                        self.frame = FrameState::NeedSymbol { sep_offset };
                        continue;
                    }
                    FrameState::NeedSymbol { sep_offset } => {
                        if matches!(byte, b'\r' | b'\n') {
                            return self.fail(Error::BadWrap { offset });
                        }
                        self.frame = FrameState::Row;
                        if let Err(error) = self.scan_symbol(byte, offset, &mut out) {
                            return self.fail(error);
                        }
                        self.symbols_in_row = 1;
                        let _ = sep_offset;
                        continue;
                    }
                    FrameState::Row => {
                        if self.symbols_in_row == width {
                            match ending {
                                LineEnding::Lf if byte == b'\n' => {
                                    self.frame = FrameState::NeedSymbol { sep_offset: offset };
                                    continue;
                                }
                                LineEnding::CrLf if byte == b'\r' => {
                                    self.frame = FrameState::NeedLf {
                                        cr_offset: offset,
                                        sep_offset: offset,
                                    };
                                    continue;
                                }
                                _ => return self.fail(Error::BadWrap { offset }),
                            }
                        }
                        if matches!(byte, b'\r' | b'\n') {
                            return self.fail(Error::BadWrap { offset });
                        }
                        if let Err(error) = self.scan_symbol(byte, offset, &mut out) {
                            return self.fail(error);
                        }
                        self.symbols_in_row += 1;
                        continue;
                    }
                }
            }

            if let Err(error) = self.scan_symbol(byte, offset, &mut out) {
                return self.fail(error);
            }
        }

        Ok(out)
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, Error> {
        if let Some(error) = &self.sticky_error {
            return Err(error.clone());
        }
        if self.finished {
            return self.fail(Error::AlreadyFinished);
        }

        match self.frame {
            FrameState::NeedLf { cr_offset, .. } => {
                return self.fail(Error::BadWrap { offset: cr_offset });
            }
            FrameState::NeedSymbol { sep_offset } => {
                return self.fail(Error::BadWrap { offset: sep_offset });
            }
            FrameState::Row => {}
        }

        if self.group.len() == 3 && self.group[2].0 == b'=' {
            return self.fail(Error::BadPadding {
                offset: self.group[2].1,
            });
        }

        let mut out = Vec::new();
        match self.group.len() {
            0 => {}
            1 => return self.fail(Error::Truncated { offset: self.group[0].1 }),
            2 => {
                if self.options.padding == Padding::Required {
                    return self.fail(Error::Truncated { offset: self.group[0].1 });
                }
                let a = digit_value(self.group[0].0, self.options.alphabet).unwrap();
                let b = digit_value(self.group[1].0, self.options.alphabet).unwrap();
                out.push((a << 2) | (b >> 4));
            }
            3 => {
                if self.options.padding == Padding::Required {
                    return self.fail(Error::Truncated { offset: self.group[0].1 });
                }
                let a = digit_value(self.group[0].0, self.options.alphabet).unwrap();
                let b = digit_value(self.group[1].0, self.options.alphabet).unwrap();
                let c = digit_value(self.group[2].0, self.options.alphabet).unwrap();
                out.push((a << 2) | (b >> 4));
                out.push((b << 4) | (c >> 2));
            }
            _ => unreachable!(),
        }

        self.group.clear();
        self.finished = true;
        Ok(out)
    }
}

fn main() {}