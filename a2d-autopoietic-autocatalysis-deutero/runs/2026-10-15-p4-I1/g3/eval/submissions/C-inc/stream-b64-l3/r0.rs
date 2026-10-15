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

fn encode_symbol(value: u8, alphabet: Alphabet) -> u8 {
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

fn encode_group(input: &[u8], options: Options, out: &mut Vec<u8>) {
    let a = input[0];
    let b = if input.len() > 1 { input[1] } else { 0 };
    let c = if input.len() > 2 { input[2] } else { 0 };

    out.push(encode_symbol(a >> 2, options.alphabet));
    out.push(encode_symbol(((a & 0x03) << 4) | (b >> 4), options.alphabet));

    if input.len() > 1 {
        out.push(encode_symbol(((b & 0x0f) << 2) | (c >> 6), options.alphabet));
    } else if options.padding != Padding::Forbidden {
        out.push(b'=');
    }

    if input.len() > 2 {
        out.push(encode_symbol(c & 0x3f, options.alphabet));
    } else if options.padding != Padding::Forbidden {
        out.push(b'=');
    }
}

pub struct Encoder {
    options: Options,
    pending: Vec<u8>,
    finished: bool,
    wrapped_count: usize,
}

impl Encoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate_options(options)?;
        Ok(Self {
            options,
            pending: Vec::new(),
            finished: false,
            wrapped_count: 0,
        })
    }

    pub fn push(&mut self, input: &[u8]) -> Result<String, Error> {
        if self.finished {
            return Err(Error::AlreadyFinished);
        }

        let mut bytes = Vec::with_capacity(self.pending.len() + input.len());
        bytes.extend_from_slice(&self.pending);
        bytes.extend_from_slice(input);

        let complete_len = bytes.len() / 3 * 3;
        let mut raw = Vec::with_capacity(complete_len / 3 * 4);
        for group in bytes[..complete_len].chunks_exact(3) {
            encode_group(group, self.options, &mut raw);
        }

        self.pending.clear();
        self.pending.extend_from_slice(&bytes[complete_len..]);
        Ok(self.format_raw(&raw))
    }

    pub fn finish(&mut self) -> Result<String, Error> {
        if self.finished {
            return Err(Error::AlreadyFinished);
        }
        self.finished = true;

        let mut raw = Vec::new();
        if !self.pending.is_empty() {
            encode_group(&self.pending, self.options, &mut raw);
            self.pending.clear();
        }
        Ok(self.format_raw(&raw))
    }

    fn format_raw(&mut self, raw: &[u8]) -> String {
        let mut formatted = Vec::with_capacity(raw.len() + 8);
        match self.options.whitespace {
            Whitespace::None | Whitespace::IgnoreAscii => {
                formatted.extend_from_slice(raw);
            }
            Whitespace::Wrapped { width, ending } => {
                for &byte in raw {
                    if self.wrapped_count > 0 && self.wrapped_count % width == 0 {
                        match ending {
                            LineEnding::Lf => formatted.push(b'\n'),
                            LineEnding::CrLf => formatted.extend_from_slice(b"\r\n"),
                        }
                    }
                    formatted.push(byte);
                    self.wrapped_count += 1;
                }
            }
        }
        String::from_utf8(formatted).expect("encoded symbols are ASCII")
    }
}

#[derive(Clone, Copy)]
enum WrapState {
    Row,
    NeedSeparator { first: usize, saw_cr: bool },
    NeedSymbol { first: usize },
}

pub struct Decoder {
    options: Options,
    group: Vec<(u8, usize)>,
    offset: usize,
    finished: bool,
    sticky: Option<Error>,
    padded: bool,
    wrap_symbols: usize,
    wrap_state: WrapState,
}

impl Decoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate_options(options)?;
        Ok(Self {
            options,
            group: Vec::with_capacity(4),
            offset: 0,
            finished: false,
            sticky: None,
            padded: false,
            wrap_symbols: 0,
            wrap_state: WrapState::Row,
        })
    }

    pub fn push(&mut self, input: &[u8]) -> Result<Vec<u8>, Error> {
        if let Some(error) = &self.sticky {
            return Err(error.clone());
        }
        if self.finished {
            return Err(Error::AlreadyFinished);
        }

        let mut out = Vec::new();
        for &byte in input {
            let at = self.offset;
            self.offset += 1;
            if let Err(error) = self.process_byte(byte, at, &mut out) {
                self.sticky = Some(error.clone());
                return Err(error);
            }
        }
        Ok(out)
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, Error> {
        if let Some(error) = &self.sticky {
            return Err(error.clone());
        }
        if self.finished {
            return Err(Error::AlreadyFinished);
        }

        if let WrapState::NeedSeparator {
            first,
            saw_cr: true,
        } = self.wrap_state
        {
            let error = Error::BadWrap { offset: first };
            self.sticky = Some(error.clone());
            return Err(error);
        }

        if let WrapState::NeedSymbol { first } = self.wrap_state {
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

        let mut out = Vec::new();
        if !self.group.is_empty() {
            let allowed = self.options.padding != Padding::Required
                && (self.group.len() == 2 || self.group.len() == 3);
            if allowed {
                decode_residual(&self.group, self.options.alphabet, &mut out);
            } else {
                let error = Error::Truncated {
                    offset: self.group[0].1,
                };
                self.sticky = Some(error.clone());
                return Err(error);
            }
        }

        self.finished = true;
        Ok(out)
    }

    fn process_byte(
        &mut self,
        byte: u8,
        at: usize,
        out: &mut Vec<u8>,
    ) -> Result<(), Error> {
        match self.options.whitespace {
            Whitespace::IgnoreAscii if is_ascii_whitespace(byte) => return Ok(()),
            Whitespace::Wrapped { width, ending } => {
                match self.wrap_state {
                    WrapState::NeedSeparator { first, saw_cr } => {
                        match ending {
                            LineEnding::Lf => {
                                if byte != b'\n' {
                                    return Err(Error::BadWrap { offset: at });
                                }
                            }
                            LineEnding::CrLf if !saw_cr => {
                                if byte != b'\r' {
                                    return Err(Error::BadWrap { offset: at });
                                }
                                self.wrap_state =
                                    WrapState::NeedSeparator { first, saw_cr: true };
                                return Ok(());
                            }
                            LineEnding::CrLf => {
                                if byte != b'\n' {
                                    return Err(Error::BadWrap { offset: at });
                                }
                            }
                        }
                        self.wrap_state = WrapState::NeedSymbol { first };
                        return Ok(());
                    }
                    WrapState::NeedSymbol { .. } => {
                        self.wrap_state = WrapState::Row;
                        self.wrap_symbols = 0;
                    }
                    WrapState::Row => {
                        if self.wrap_symbols == width {
                            self.wrap_state = WrapState::NeedSeparator {
                                first: at,
                                saw_cr: false,
                            };
                            return self.process_byte(byte, at, out);
                        }
                        if is_ascii_whitespace(byte) {
                            return Err(Error::BadWrap { offset: at });
                        }
                    }
                }
            }
            _ => {}
        }

        if self.padded {
            return Err(Error::BadPadding { offset: at });
        }

        if self.group.len() == 3 && self.group[2].0 == b'=' && byte != b'=' {
            return Err(Error::BadPadding { offset: at });
        }

        let pos = self.group.len();
        if byte == b'=' {
            if pos == 0 || pos == 1 {
                return Err(Error::BadPadding { offset: at });
            }
            if self.options.padding == Padding::Forbidden {
                return Err(Error::BadPadding { offset: at });
            }

            self.group.push((byte, at));
            if pos == 3 {
                decode_group(&self.group, self.options.alphabet, out);
                self.group.clear();
                self.padded = true;
            }
        } else {
            let value = digit_value(byte, self.options.alphabet)
                .ok_or(Error::InvalidByte { offset: at, byte })?;
            self.group.push((byte, at));
            if self.group.len() == 4 {
                decode_values(
                    digit_value(self.group[0].0, self.options.alphabet).unwrap(),
                    digit_value(self.group[1].0, self.options.alphabet).unwrap(),
                    digit_value(self.group[2].0, self.options.alphabet).unwrap(),
                    value,
                    out,
                );
                self.group.clear();
            }
        }

        if matches!(self.options.whitespace, Whitespace::Wrapped { .. }) {
            self.wrap_symbols += 1;
        }
        Ok(())
    }
}

fn is_ascii_whitespace(byte: u8) -> bool {
    matches!(byte, 9 | 10 | 11 | 12 | 13 | 32)
}

fn decode_values(a: u8, b: u8, c: u8, d: u8, out: &mut Vec<u8>) {
    out.push((a << 2) | (b >> 4));
    out.push((b << 4) | (c >> 2));
    out.push((c << 6) | d);
}

fn decode_group(group: &[(u8, usize)], alphabet: Alphabet, out: &mut Vec<u8>) {
    let a = digit_value(group[0].0, alphabet).unwrap();
    let b = digit_value(group[1].0, alphabet).unwrap();
    out.push((a << 2) | (b >> 4));

    if group[2].0 != b'=' {
        let c = digit_value(group[2].0, alphabet).unwrap();
        out.push((b << 4) | (c >> 2));
        if group[3].0 != b'=' {
            let d = digit_value(group[3].0, alphabet).unwrap();
            out.push((c << 6) | d);
        }
    }
}

fn decode_residual(group: &[(u8, usize)], alphabet: Alphabet, out: &mut Vec<u8>) {
    let a = digit_value(group[0].0, alphabet).unwrap();
    let b = digit_value(group[1].0, alphabet).unwrap();
    out.push((a << 2) | (b >> 4));

    if group.len() == 3 {
        let c = digit_value(group[2].0, alphabet).unwrap();
        out.push((b << 4) | (c >> 2));
    }
}

pub fn encode(input: &[u8], options: Options) -> Result<String, Error> {
    validate_options(options)?;
    let mut encoder = Encoder::new(options)?;
    let mut result = encoder.push(input)?;
    result.push_str(&encoder.finish()?);
    Ok(result)
}

pub fn decode(input: &[u8], options: Options) -> Result<Vec<u8>, Error> {
    validate_options(options)?;
    let mut decoder = Decoder::new(options)?;
    let mut result = decoder.push(input)?;
    result.extend(decoder.finish()?);
    Ok(result)
}

fn main() {}