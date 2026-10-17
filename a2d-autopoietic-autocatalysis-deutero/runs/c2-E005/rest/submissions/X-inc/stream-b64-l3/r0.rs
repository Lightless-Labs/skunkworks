use std::fmt;

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

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for Error {}

fn validate(options: Options) -> Result<(), Error> {
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

fn encode_group(input: &[u8], alphabet: Alphabet, padding: Padding) -> String {
    let table = alphabet_bytes(alphabet);
    let a = input[0];
    let b = *input.get(1).unwrap_or(&0);
    let c = *input.get(2).unwrap_or(&0);

    let mut out = String::with_capacity(4);
    out.push(table[(a >> 2) as usize] as char);
    out.push(table[(((a & 0x03) << 4) | (b >> 4)) as usize] as char);

    if input.len() >= 2 {
        out.push(table[(((b & 0x0f) << 2) | (c >> 6)) as usize] as char);
    } else if padding != Padding::Forbidden {
        out.push('=');
    }

    if input.len() >= 3 {
        out.push(table[(c & 0x3f) as usize] as char);
    } else if padding != Padding::Forbidden {
        out.push('=');
    }
    out
}

fn apply_wrapping(raw: &str, whitespace: Whitespace) -> String {
    let Whitespace::Wrapped { width, ending } = whitespace else {
        return raw.to_owned();
    };
    if raw.is_empty() {
        return String::new();
    }

    let separator = match ending {
        LineEnding::Lf => "\n",
        LineEnding::CrLf => "\r\n",
    };
    let mut out = String::with_capacity(raw.len() + raw.len() / width * separator.len());
    for (i, chunk) in raw.as_bytes().chunks(width).enumerate() {
        if i > 0 {
            out.push_str(separator);
        }
        out.push_str(std::str::from_utf8(chunk).expect("base64 output is ASCII"));
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

pub struct Encoder {
    options: Options,
    buffered: Vec<u8>,
    raw_symbols: usize,
    closed: bool,
}

impl Encoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate(options)?;
        Ok(Self {
            options,
            buffered: Vec::with_capacity(2),
            raw_symbols: 0,
            closed: false,
        })
    }

    pub fn push(&mut self, input: &[u8]) -> Result<String, Error> {
        if self.closed {
            return Err(Error::AlreadyFinished);
        }

        let mut data = Vec::with_capacity(self.buffered.len() + input.len());
        data.extend_from_slice(&self.buffered);
        data.extend_from_slice(input);

        let complete_len = data.len() / 3 * 3;
        let mut raw = String::with_capacity(complete_len / 3 * 4);
        for group in data[..complete_len].chunks_exact(3) {
            raw.push_str(&encode_group(group, self.options.alphabet, self.options.padding));
        }

        self.buffered.clear();
        self.buffered.extend_from_slice(&data[complete_len..]);
        let output = match self.options.whitespace {
            Whitespace::Wrapped { width, ending } => {
                let separator = match ending {
                    LineEnding::Lf => "\n",
                    LineEnding::CrLf => "\r\n",
                };
                let mut framed = String::new();
                for byte in raw.bytes() {
                    if self.raw_symbols > 0 && self.raw_symbols % width == 0 {
                        framed.push_str(separator);
                    }
                    framed.push(byte as char);
                    self.raw_symbols += 1;
                }
                framed
            }
            _ => raw,
        };
        Ok(output)
    }

    pub fn finish(&mut self) -> Result<String, Error> {
        if self.closed {
            return Err(Error::AlreadyFinished);
        }
        self.closed = true;

        let mut raw = if self.buffered.is_empty() {
            String::new()
        } else {
            encode_group(
                &self.buffered,
                self.options.alphabet,
                self.options.padding,
            )
        };
        self.buffered.clear();

        if let Whitespace::Wrapped { width, ending } = self.options.whitespace {
            let separator = match ending {
                LineEnding::Lf => "\n",
                LineEnding::CrLf => "\r\n",
            };
            let mut framed = String::new();
            for byte in raw.bytes() {
                if self.raw_symbols > 0 && self.raw_symbols % width == 0 {
                    framed.push_str(separator);
                }
                framed.push(byte as char);
                self.raw_symbols += 1;
            }
            raw = framed;
        }
        Ok(raw)
    }
}

#[derive(Clone, Copy)]
enum FramingState {
    Row,
    NeedLf { cr_offset: usize, separator_offset: usize },
    NeedSymbol { separator_offset: usize },
}

pub struct Decoder {
    options: Options,
    offset: usize,
    group: [u8; 4],
    group_offsets: [usize; 4],
    group_len: usize,
    pending_second_padding: Option<usize>,
    terminal_padding: bool,
    symbols_in_row: usize,
    framing: FramingState,
    closed: bool,
    sticky_error: Option<Error>,
}

fn is_ignored(byte: u8) -> bool {
    matches!(byte, 9 | 10 | 11 | 12 | 13 | 32)
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

fn decode_group(
    bytes: &[u8; 4],
    offsets: &[usize; 4],
    alphabet: Alphabet,
    padding: Padding,
) -> Result<Vec<u8>, Error> {
    let vals: [u8; 4] = [
        digit_value(bytes[0], alphabet).ok_or(Error::InvalidByte {
            offset: offsets[0],
            byte: bytes[0],
        })?,
        digit_value(bytes[1], alphabet).ok_or(Error::InvalidByte {
            offset: offsets[1],
            byte: bytes[1],
        })?,
        if bytes[2] == b'=' {
            0
        } else {
            digit_value(bytes[2], alphabet).ok_or(Error::InvalidByte {
                offset: offsets[2],
                byte: bytes[2],
            })?
        },
        if bytes[3] == b'=' {
            0
        } else {
            digit_value(bytes[3], alphabet).ok_or(Error::InvalidByte {
                offset: offsets[3],
                byte: bytes[3],
            })?
        },
    ];

    let mut out = Vec::with_capacity(3);
    out.push((vals[0] << 2) | (vals[1] >> 4));
    if bytes[2] != b'=' {
        out.push((vals[1] << 4) | (vals[2] >> 2));
        if bytes[3] != b'=' {
            out.push((vals[2] << 6) | vals[3]);
        }
    }

    if padding == Padding::Forbidden && (bytes[2] == b'=' || bytes[3] == b'=') {
        return Err(Error::BadPadding {
            offset: if bytes[2] == b'=' { offsets[2] } else { offsets[3] },
        });
    }
    Ok(out)
}

impl Decoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate(options)?;
        Ok(Self {
            options,
            offset: 0,
            group: [0; 4],
            group_offsets: [0; 4],
            group_len: 0,
            pending_second_padding: None,
            terminal_padding: false,
            symbols_in_row: 0,
            framing: FramingState::Row,
            closed: false,
            sticky_error: None,
        })
    }

    fn fail(&mut self, error: Error) -> Error {
        self.sticky_error = Some(error.clone());
        error
    }

    fn process_symbol(&mut self, byte: u8, offset: usize, out: &mut Vec<u8>) -> Result<(), Error> {
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
            match self.group_len {
                0 | 1 => return Err(Error::BadPadding { offset }),
                2 => {
                    self.group[2] = b'=';
                    self.group_offsets[2] = offset;
                    self.group_len = 3;
                    self.pending_second_padding = Some(offset);
                    return Ok(());
                }
                3 => {
                    self.group[3] = b'=';
                    self.group_offsets[3] = offset;
                    self.group_len = 4;
                    let decoded = decode_group(
                        &self.group,
                        &self.group_offsets,
                        self.options.alphabet,
                        self.options.padding,
                    )?;
                    out.extend(decoded);
                    self.group_len = 0;
                    self.pending_second_padding = None;
                    self.terminal_padding = true;
                    return Ok(());
                }
                _ => return Err(Error::BadPadding { offset }),
            }
        }

        let value = digit_value(byte, self.options.alphabet).ok_or(Error::InvalidByte {
            offset,
            byte,
        })?;
        self.group[self.group_len] = byte;
        self.group_offsets[self.group_len] = offset;
        self.group_len += 1;

        if self.group_len == 4 {
            let decoded = decode_group(
                &self.group,
                &self.group_offsets,
                self.options.alphabet,
                self.options.padding,
            )?;
            out.extend(decoded);
            self.group_len = 0;
        }

        let _ = value;
        Ok(())
    }

    pub fn push(&mut self, input: &[u8]) -> Result<Vec<u8>, Error> {
        if let Some(error) = &self.sticky_error {
            return Err(error.clone());
        }
        if self.closed {
            return Err(Error::AlreadyFinished);
        }

        let mut out = Vec::new();
        for &byte in input {
            let physical_offset = self.offset;
            self.offset += 1;

            if self.options.whitespace == Whitespace::IgnoreAscii && is_ignored(byte) {
                continue;
            }

            match self.options.whitespace {
                Whitespace::Wrapped { width, ending } => {
                    let symbol_at_row_limit = self.symbols_in_row == width;
                    match self.framing {
                        FramingState::NeedLf { cr_offset, separator_offset } => {
                            if byte != b'\n' {
                                return Err(self.fail(Error::BadWrap {
                                    offset: physical_offset,
                                }));
                            }
                            self.framing = FramingState::NeedSymbol { separator_offset };
                            let _ = cr_offset;
                            continue;
                        }
                        FramingState::NeedSymbol { separator_offset } => {
                            if is_ignored(byte) {
                                return Err(self.fail(Error::BadWrap { offset: physical_offset }));
                            }
                            self.framing = FramingState::Row;
                            self.symbols_in_row = 0;
                            let _ = separator_offset;
                        }
                        FramingState::Row if symbol_at_row_limit => {
                            match ending {
                                LineEnding::Lf => {
                                    if byte != b'\n' {
                                        return Err(self.fail(Error::BadWrap {
                                            offset: physical_offset,
                                        }));
                                    }
                                    self.framing = FramingState::NeedSymbol {
                                        separator_offset: physical_offset,
                                    };
                                    continue;
                                }
                                LineEnding::CrLf => {
                                    if byte != b'\r' {
                                        return Err(self.fail(Error::BadWrap {
                                            offset: physical_offset,
                                        }));
                                    }
                                    self.framing = FramingState::NeedLf {
                                        cr_offset: physical_offset,
                                        separator_offset: physical_offset,
                                    };
                                    continue;
                                }
                            }
                        }
                        FramingState::Row => {
                            if is_ignored(byte) {
                                return Err(self.fail(Error::BadWrap { offset: physical_offset }));
                            }
                        }
                    }

                    if let Err(error) = self.process_symbol(byte, physical_offset, &mut out) {
                        return Err(self.fail(error));
                    }
                    self.symbols_in_row += 1;
                }
                _ => {
                    if let Err(error) = self.process_symbol(byte, physical_offset, &mut out) {
                        return Err(self.fail(error));
                    }
                }
            }
        }
        Ok(out)
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, Error> {
        if let Some(error) = &self.sticky_error {
            return Err(error.clone());
        }
        if self.closed {
            return Err(Error::AlreadyFinished);
        }

        if let Whitespace::Wrapped { .. } = self.options.whitespace {
            match self.framing {
                FramingState::NeedLf { cr_offset, .. } => {
                    return Err(self.fail(Error::BadWrap { offset: cr_offset }));
                }
                FramingState::NeedSymbol { separator_offset } => {
                    return Err(self.fail(Error::BadWrap {
                        offset: separator_offset,
                    }));
                }
                FramingState::Row => {}
            }
        }

        let result = if self.pending_second_padding.is_some() {
            Err(Error::BadPadding {
                offset: self.pending_second_padding.unwrap(),
            })
        } else if self.group_len == 0 {
            Ok(Vec::new())
        } else if self.options.padding == Padding::Required {
            Err(Error::Truncated {
                offset: self.group_offsets[0],
            })
        } else {
            match self.group_len {
                1 => Err(Error::Truncated {
                    offset: self.group_offsets[0],
                }),
                2 => {
                    let a = digit_value(self.group[0], self.options.alphabet).unwrap();
                    let b = digit_value(self.group[1], self.options.alphabet).unwrap();
                    Ok(vec![(a << 2) | (b >> 4)])
                }
                3 => {
                    let a = digit_value(self.group[0], self.options.alphabet).unwrap();
                    let b = digit_value(self.group[1], self.options.alphabet).unwrap();
                    let c = digit_value(self.group[2], self.options.alphabet).unwrap();
                    Ok(vec![(a << 2) | (b >> 4), (b << 4) | (c >> 2)])
                }
                _ => Ok(Vec::new()),
            }
        };

        match result {
            Ok(output) => {
                self.closed = true;
                Ok(output)
            }
            Err(error) => Err(self.fail(error)),
        }
    }
}

pub fn decode(input: &[u8], options: Options) -> Result<Vec<u8>, Error> {
    validate(options)?;
    let mut decoder = Decoder::new(options)?;
    let mut out = decoder.push(input)?;
    out.extend(decoder.finish()?);
    Ok(out)
}