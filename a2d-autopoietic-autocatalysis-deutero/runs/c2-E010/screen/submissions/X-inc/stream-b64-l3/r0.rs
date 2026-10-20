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

fn digit_byte(digit: u8, alphabet: Alphabet) -> u8 {
    match digit {
        0..=25 => b'A' + digit,
        26..=51 => b'a' + (digit - 26),
        52..=61 => b'0' + (digit - 52),
        62 if alphabet == Alphabet::Standard => b'+',
        63 if alphabet == Alphabet::Standard => b'/',
        62 => b'-',
        _ => b'_',
    }
}

fn encode_block(input: &[u8], alphabet: Alphabet, padding: Padding) -> String {
    let mut output = String::with_capacity(4);
    let a = input[0];
    let b = if input.len() > 1 { input[1] } else { 0 };
    let c = if input.len() > 2 { input[2] } else { 0 };

    let digits = [
        a >> 2,
        ((a & 0x03) << 4) | (b >> 4),
        ((b & 0x0f) << 2) | (c >> 6),
        c & 0x3f,
    ];

    output.push(digit_byte(digits[0], alphabet) as char);
    output.push(digit_byte(digits[1], alphabet) as char);

    if input.len() >= 2 {
        output.push(digit_byte(digits[2], alphabet) as char);
    } else if padding != Padding::Forbidden {
        output.push('=');
    }

    if input.len() >= 3 {
        output.push(digit_byte(digits[3], alphabet) as char);
    } else if padding != Padding::Forbidden {
        output.push('=');
    }

    output
}

fn wrap_symbols(raw: String, whitespace: Whitespace) -> String {
    let Whitespace::Wrapped { width, ending } = whitespace else {
        return raw;
    };

    let separator = match ending {
        LineEnding::Lf => "\n",
        LineEnding::CrLf => "\r\n",
    };

    let mut output = String::with_capacity(raw.len() + raw.len() / width * separator.len());
    for (i, byte) in raw.bytes().enumerate() {
        if i != 0 && i % width == 0 {
            output.push_str(separator);
        }
        output.push(byte as char);
    }
    output
}

pub fn encode(input: &[u8], options: Options) -> Result<String, Error> {
    validate(options)?;
    let mut encoder = Encoder::new(options)?;
    let mut output = encoder.push(input)?;
    output.push_str(&encoder.finish()?);
    Ok(output)
}

pub struct Encoder {
    options: Options,
    pending: Vec<u8>,
    closed: bool,
}

impl Encoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate(options)?;
        Ok(Self {
            options,
            pending: Vec::with_capacity(2),
            closed: false,
        })
    }

    pub fn push(&mut self, input: &[u8]) -> Result<String, Error> {
        if self.closed {
            return Err(Error::AlreadyFinished);
        }

        let mut data = Vec::with_capacity(self.pending.len() + input.len());
        data.extend_from_slice(&self.pending);
        data.extend_from_slice(input);

        let complete_len = data.len() / 3 * 3;
        let mut raw = String::with_capacity(complete_len / 3 * 4);
        for block in data[..complete_len].chunks_exact(3) {
            raw.push_str(&encode_block(block, self.options.alphabet, self.options.padding));
        }

        self.pending.clear();
        self.pending.extend_from_slice(&data[complete_len..]);
        Ok(raw)
    }

    pub fn finish(&mut self) -> Result<String, Error> {
        if self.closed {
            return Err(Error::AlreadyFinished);
        }
        self.closed = true;

        let raw = if self.pending.is_empty() {
            String::new()
        } else {
            encode_block(&self.pending, self.options.alphabet, self.options.padding)
        };
        self.pending.clear();
        Ok(wrap_symbols(raw, self.options.whitespace))
    }
}

#[derive(Clone, Copy)]
enum FrameState {
    Normal,
    NeedLf { cr_offset: usize, first: usize },
    NeedSymbol { first: usize },
}

pub struct Decoder {
    options: Options,
    group: Vec<(u8, usize)>,
    next_offset: usize,
    symbols_in_row: usize,
    frame: FrameState,
    padded: bool,
    closed: bool,
    sticky: Option<Error>,
}

impl Decoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate(options)?;
        Ok(Self {
            options,
            group: Vec::with_capacity(4),
            next_offset: 0,
            symbols_in_row: 0,
            frame: FrameState::Normal,
            padded: false,
            closed: false,
            sticky: None,
        })
    }

    fn scan_symbol(&mut self, byte: u8, offset: usize, output: &mut Vec<u8>) -> Result<(), Error> {
        if self.padded {
            return Err(Error::BadPadding { offset });
        }

        if self.group.len() == 2 && self.group[1].0 == b'=' && byte != b'=' {
            return Err(Error::BadPadding { offset });
        }

        let position = self.group.len();
        if byte == b'=' {
            if position < 2 {
                return Err(Error::BadPadding { offset });
            }
            if self.options.padding == Padding::Forbidden {
                return Err(Error::BadPadding { offset });
            }
        } else if alphabet_digit(byte, self.options.alphabet).is_none() {
            return Err(Error::InvalidByte { offset, byte });
        }

        self.group.push((byte, offset));
        if self.group.len() == 4 {
            self.emit_group(output)?;
        }
        Ok(())
    }

    fn emit_group(&mut self, output: &mut Vec<u8>) -> Result<(), Error> {
        let g = &self.group;
        let d0 = alphabet_digit(g[0].0, self.options.alphabet).unwrap();
        let d1 = alphabet_digit(g[1].0, self.options.alphabet).unwrap();

        if g[2].0 == b'=' {
            if g[3].0 != b'=' {
                return Err(Error::BadPadding { offset: g[3].1 });
            }
            output.push((d0 << 2) | (d1 >> 4));
            self.padded = true;
        } else {
            let d2 = alphabet_digit(g[2].0, self.options.alphabet).unwrap();
            if g[3].0 == b'=' {
                output.push((d0 << 2) | (d1 >> 4));
                output.push((d1 << 4) | (d2 >> 2));
                self.padded = true;
            } else {
                let d3 = alphabet_digit(g[3].0, self.options.alphabet).unwrap();
                output.push((d0 << 2) | (d1 >> 4));
                output.push((d1 << 4) | (d2 >> 2));
                output.push((d2 << 6) | d3);
            }
        }

        self.group.clear();
        Ok(())
    }

    fn scan_wrapped_byte(
        &mut self,
        byte: u8,
        offset: usize,
        output: &mut Vec<u8>,
    ) -> Result<(), Error> {
        let Whitespace::Wrapped { width, ending } = self.options.whitespace else {
            return self.scan_symbol(byte, offset, output);
        };

        match self.frame {
            FrameState::NeedLf { cr_offset, first } => {
                if byte != b'\n' {
                    return Err(Error::BadWrap { offset });
                }
                self.frame = FrameState::NeedSymbol { first };
                let _ = cr_offset;
                return Ok(());
            }
            FrameState::NeedSymbol { first } => {
                self.frame = FrameState::Normal;
                self.symbols_in_row = 0;
                if byte == b'\r' || byte == b'\n' || byte == b' ' || byte == b'\t' {
                    return Err(Error::BadWrap { offset });
                }
                let _ = first;
            }
            FrameState::Normal => {}
        }

        if self.symbols_in_row == width {
            match ending {
                LineEnding::Lf if byte == b'\n' => {
                    self.frame = FrameState::NeedSymbol { first: offset };
                    return Ok(());
                }
                LineEnding::CrLf if byte == b'\r' => {
                    self.frame = FrameState::NeedLf {
                        cr_offset: offset,
                        first: offset,
                    };
                    return Ok(());
                }
                _ => return Err(Error::BadWrap { offset }),
            }
        }

        if byte == b'\r' || byte == b'\n' || byte == b' ' || byte == b'\t' {
            return Err(Error::BadWrap { offset });
        }

        self.symbols_in_row += 1;
        self.scan_symbol(byte, offset, output)
    }

    pub fn push(&mut self, input: &[u8]) -> Result<Vec<u8>, Error> {
        if let Some(error) = &self.sticky {
            return Err(error.clone());
        }
        if self.closed {
            return Err(Error::AlreadyFinished);
        }

        let mut output = Vec::new();
        for &byte in input {
            let offset = self.next_offset;
            self.next_offset += 1;

            let result = match self.options.whitespace {
                Whitespace::None => self.scan_symbol(byte, offset, &mut output),
                Whitespace::IgnoreAscii => {
                    if matches!(byte, 9 | 10 | 11 | 12 | 13 | 32) {
                        Ok(())
                    } else {
                        self.scan_symbol(byte, offset, &mut output)
                    }
                }
                Whitespace::Wrapped { .. } => {
                    self.scan_wrapped_byte(byte, offset, &mut output)
                }
            };

            if let Err(error) = result {
                self.sticky = Some(error.clone());
                return Err(error);
            }
        }
        Ok(output)
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, Error> {
        if let Some(error) = &self.sticky {
            return Err(error.clone());
        }
        if self.closed {
            return Err(Error::AlreadyFinished);
        }

        if let FrameState::NeedLf { cr_offset, .. } = self.frame {
            let error = Error::BadWrap { offset: cr_offset };
            self.sticky = Some(error.clone());
            return Err(error);
        }
        if let FrameState::NeedSymbol { first } = self.frame {
            let error = Error::BadWrap { offset: first };
            self.sticky = Some(error.clone());
            return Err(error);
        }

        if self.group.len() == 3 && self.group[2].0 == b'=' {
            let error = Error::BadPadding {
                offset: self.group[2].1,
            };
            self.sticky = Some(error.clone());
            return Err(error);
        }

        let mut output = Vec::new();
        match self.group.len() {
            0 => {}
            1 => {
                let error = Error::Truncated {
                    offset: self.group[0].1,
                };
                self.sticky = Some(error.clone());
                return Err(error);
            }
            2 | 3 if self.options.padding != Padding::Required => {
                let d0 = alphabet_digit(self.group[0].0, self.options.alphabet).unwrap();
                let d1 = alphabet_digit(self.group[1].0, self.options.alphabet).unwrap();
                output.push((d0 << 2) | (d1 >> 4));
                if self.group.len() == 3 {
                    let d2 = alphabet_digit(self.group[2].0, self.options.alphabet).unwrap();
                    output.push((d1 << 4) | (d2 >> 2));
                }
            }
            _ => {
                let error = Error::Truncated {
                    offset: self.group[0].1,
                };
                self.sticky = Some(error.clone());
                return Err(error);
            }
        }

        self.group.clear();
        self.closed = true;
        Ok(output)
    }
}

pub fn decode(input: &[u8], options: Options) -> Result<Vec<u8>, Error> {
    validate(options)?;
    let mut decoder = Decoder::new(options)?;
    let mut output = decoder.push(input)?;
    output.extend(decoder.finish()?);
    Ok(output)
}