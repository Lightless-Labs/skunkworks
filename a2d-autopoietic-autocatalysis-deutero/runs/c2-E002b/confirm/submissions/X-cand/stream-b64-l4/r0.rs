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

fn alphabet_value(byte: u8, alphabet: Alphabet) -> Option<u8> {
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

fn foreign_alphabet_symbol(byte: u8, alphabet: Alphabet) -> bool {
    match alphabet {
        Alphabet::Standard => byte == b'-' || byte == b'_',
        Alphabet::UrlSafe => byte == b'+' || byte == b'/',
    }
}

fn encode_group(bytes: &[u8], alphabet: Alphabet, padding: Padding) -> Vec<u8> {
    let a = bytes[0];
    let b = if bytes.len() > 1 { bytes[1] } else { 0 };
    let c = if bytes.len() > 2 { bytes[2] } else { 0 };

    let mut out = vec![
        encode_digit(a >> 2, alphabet),
        encode_digit(((a & 0x03) << 4) | (b >> 4), alphabet),
    ];

    if bytes.len() > 1 {
        out.push(encode_digit(((b & 0x0f) << 2) | (c >> 6), alphabet));
    } else if padding != Padding::Forbidden {
        out.push(b'=');
    }

    if bytes.len() > 2 {
        out.push(encode_digit(c & 0x3f, alphabet));
    } else if padding != Padding::Forbidden {
        out.push(b'=');
    }

    out
}

pub fn encode(input: &[u8], options: Options) -> Result<String, Error> {
    validate(options)?;
    let mut encoder = Encoder::new(options)?;
    let mut output = encoder.push(input)?;
    output.push_str(&encoder.finish()?);
    Ok(output)
}

pub fn decode(input: &[u8], options: Options) -> Result<Vec<u8>, Error> {
    validate(options)?;
    let mut decoder = Decoder::new(options)?;
    let mut output = decoder.push(input)?;
    output.extend(decoder.finish()?);
    Ok(output)
}

pub struct Encoder {
    options: Options,
    tail: Vec<u8>,
    closed: bool,
    row_symbols: usize,
    pending_separator: bool,
}

impl Encoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate(options)?;
        Ok(Self {
            options,
            tail: Vec::new(),
            closed: false,
            row_symbols: 0,
            pending_separator: false,
        })
    }

    fn append_symbols(&mut self, symbols: &[u8], out: &mut String) {
        let wrapped = match self.options.whitespace {
            Whitespace::Wrapped { width, .. } => Some(width),
            _ => None,
        };
        for &symbol in symbols {
            if let Some(width) = wrapped {
                if self.pending_separator {
                    match self.options.whitespace {
                        Whitespace::Wrapped {
                            ending: LineEnding::Lf,
                            ..
                        } => out.push('\n'),
                        Whitespace::Wrapped {
                            ending: LineEnding::CrLf,
                            ..
                        } => out.push_str("\r\n"),
                        _ => {}
                    }
                    self.pending_separator = false;
                    self.row_symbols = 0;
                }
                out.push(symbol as char);
                self.row_symbols += 1;
                if self.row_symbols == width {
                    self.pending_separator = true;
                }
            } else {
                out.push(symbol as char);
            }
        }
    }

    pub fn push(&mut self, input: &[u8]) -> Result<String, Error> {
        if self.closed {
            return Err(Error::AlreadyFinished);
        }

        let mut bytes = Vec::with_capacity(self.tail.len() + input.len());
        bytes.extend_from_slice(&self.tail);
        bytes.extend_from_slice(input);
        self.tail.clear();

        let complete_len = bytes.len() / 3 * 3;
        let mut output = String::new();
        for group in bytes[..complete_len].chunks_exact(3) {
            let symbols = encode_group(group, self.options.alphabet, self.options.padding);
            self.append_symbols(&symbols, &mut output);
        }
        self.tail.extend_from_slice(&bytes[complete_len..]);
        Ok(output)
    }

    pub fn finish(&mut self) -> Result<String, Error> {
        if self.closed {
            return Err(Error::AlreadyFinished);
        }
        self.closed = true;
        let mut output = String::new();
        if !self.tail.is_empty() {
            let symbols = encode_group(
                &self.tail,
                self.options.alphabet,
                self.options.padding,
            );
            self.append_symbols(&symbols, &mut output);
            self.tail.clear();
        }
        Ok(output)
    }
}

#[derive(Clone, Copy)]
enum WrapState {
    Row,
    NeedLf { cr_offset: usize },
    NeedSymbol { separator_offset: usize },
}

pub struct Decoder {
    options: Options,
    offset: usize,
    symbols_seen: usize,
    group: Vec<(u8, usize)>,
    pending_second_pad: Option<usize>,
    padded_finished: bool,
    closed: bool,
    sticky_error: Option<Error>,
    wrap_symbols: usize,
    wrap_state: WrapState,
}

impl Decoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate(options)?;
        Ok(Self {
            options,
            offset: 0,
            symbols_seen: 0,
            group: Vec::with_capacity(4),
            pending_second_pad: None,
            padded_finished: false,
            closed: false,
            sticky_error: None,
            wrap_symbols: 0,
            wrap_state: WrapState::Row,
        })
    }

    fn fail(&mut self, error: Error) -> Error {
        self.sticky_error = Some(error.clone());
        error
    }

    fn is_ignored(byte: u8) -> bool {
        matches!(byte, 9 | 10 | 11 | 12 | 13 | 32)
    }

    fn process_symbol(
        &mut self,
        byte: u8,
        offset: usize,
        output: &mut Vec<u8>,
    ) -> Result<(), Error> {
        if let Some(limit) = self.options.max_symbols {
            if self.symbols_seen >= limit {
                return Err(Error::Overlong { offset, limit });
            }
        }
        self.symbols_seen += 1;

        if self.padded_finished {
            return Err(Error::BadPadding { offset });
        }

        if let Some(pad_offset) = self.pending_second_pad {
            if byte != b'=' {
                return Err(Error::BadPadding { offset });
            }
            self.group.push((byte, offset));
            self.pending_second_pad = None;
            self.finish_padded_group(pad_offset, output)?;
            return Ok(());
        }

        match self.group.len() {
            0 | 1 if byte == b'=' => Err(Error::BadPadding { offset }),
            2 if byte == b'=' => {
                self.group.push((byte, offset));
                self.pending_second_pad = Some(offset);
                Ok(())
            }
            _ if byte == b'=' => {
                if self.options.padding == Padding::Forbidden {
                    return Err(Error::BadPadding { offset });
                }
                self.group.push((byte, offset));
                if self.group.len() == 4 {
                    self.finish_padded_group(offset, output)?;
                }
                Ok(())
            }
            _ => {
                if let Some(value) = alphabet_value(byte, self.options.alphabet) {
                    self.group.push((byte, offset));
                    if self.group.len() == 4 {
                        let values = [
                            alphabet_value(self.group[0].0, self.options.alphabet).unwrap(),
                            alphabet_value(self.group[1].0, self.options.alphabet).unwrap(),
                            alphabet_value(self.group[2].0, self.options.alphabet).unwrap(),
                            alphabet_value(self.group[3].0, self.options.alphabet).unwrap(),
                        ];
                        output.push((values[0] << 2) | (values[1] >> 4));
                        output.push((values[1] << 4) | (values[2] >> 2));
                        output.push((values[2] << 6) | values[3]);
                        self.group.clear();
                    }
                    let _ = value;
                    Ok(())
                } else if self.options.strict
                    && foreign_alphabet_symbol(byte, self.options.alphabet)
                {
                    Err(Error::MixedAlphabet { offset })
                } else {
                    Err(Error::InvalidByte { offset, byte })
                }
            }
        }
    }

    fn finish_padded_group(
        &mut self,
        pad_offset: usize,
        output: &mut Vec<u8>,
    ) -> Result<(), Error> {
        if self.group.len() != 4 {
            return Ok(());
        }
        let first = alphabet_value(self.group[0].0, self.options.alphabet)
            .ok_or(Error::InvalidByte {
                offset: self.group[0].1,
                byte: self.group[0].0,
            })?;
        let second = alphabet_value(self.group[1].0, self.options.alphabet)
            .ok_or(Error::InvalidByte {
                offset: self.group[1].1,
                byte: self.group[1].0,
            })?;

        if self.group[2].0 == b'=' {
            if self.options.strict && (second & 0x0f) != 0 {
                return Err(Error::NonCanonical {
                    offset: self.group[1].1,
                });
            }
            output.push((first << 2) | (second >> 4));
        } else {
            let third = alphabet_value(self.group[2].0, self.options.alphabet)
                .ok_or(Error::InvalidByte {
                    offset: self.group[2].1,
                    byte: self.group[2].0,
                })?;
            if self.options.strict && (third & 0x03) != 0 {
                return Err(Error::NonCanonical {
                    offset: self.group[2].1,
                });
            }
            output.push((first << 2) | (second >> 4));
            output.push((second << 4) | (third >> 2));
        }
        self.group.clear();
        self.padded_finished = true;
        let _ = pad_offset;
        Ok(())
    }

    fn process_byte(
        &mut self,
        byte: u8,
        offset: usize,
        output: &mut Vec<u8>,
    ) -> Result<(), Error> {
        if self.options.whitespace == Whitespace::IgnoreAscii && Self::is_ignored(byte) {
            return Ok(());
        }

        if let Whitespace::Wrapped { width, ending } = self.options.whitespace {
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
                WrapState::NeedSymbol { separator_offset } => {
                    self.wrap_state = WrapState::Row;
                    self.wrap_symbols = 0;
                    if byte == b'\n' || byte == b'\r' {
                        return Err(Error::BadWrap { offset });
                    }
                    self.process_symbol(byte, offset, output)?;
                    self.wrap_symbols += 1;
                    let _ = separator_offset;
                    return Ok(());
                }
                WrapState::Row => {
                    if self.wrap_symbols == width {
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
                    if byte == b'\n' || byte == b'\r' || Self::is_ignored(byte) {
                        return Err(Error::BadWrap { offset });
                    }
                    self.process_symbol(byte, offset, output)?;
                    self.wrap_symbols += 1;
                    Ok(())
                }
            }
        } else {
            self.process_symbol(byte, offset, output)
        }
    }

    pub fn push(&mut self, input: &[u8]) -> Result<Vec<u8>, Error> {
        if let Some(error) = &self.sticky_error {
            return Err(error.clone());
        }
        if self.closed {
            let error = Error::AlreadyFinished;
            self.sticky_error = Some(error.clone());
            return Err(error);
        }

        let mut output = Vec::new();
        for &byte in input {
            let offset = self.offset;
            self.offset += 1;
            if let Err(error) = self.process_byte(byte, offset, &mut output) {
                return Err(self.fail(error));
            }
        }
        Ok(output)
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, Error> {
        if let Some(error) = &self.sticky_error {
            return Err(error.clone());
        }
        if self.closed {
            let error = Error::AlreadyFinished;
            self.sticky_error = Some(error.clone());
            return Err(error);
        }

        let mut output = Vec::new();

        match self.wrap_state {
            WrapState::NeedLf { cr_offset } => {
                return Err(self.fail(Error::BadWrap { offset: cr_offset }));
            }
            WrapState::NeedSymbol { separator_offset } => {
                return Err(self.fail(Error::BadWrap {
                    offset: separator_offset,
                }));
            }
            WrapState::Row => {}
        }

        if self.pending_second_pad.is_some() {
            return Err(self.fail(Error::BadPadding {
                offset: self.pending_second_pad.unwrap(),
            }));
        }

        if !self.group.is_empty() {
            match self.options.padding {
                Padding::Required => {
                    return Err(self.fail(Error::Truncated {
                        offset: self.group[0].1,
                    }));
                }
                Padding::Forbidden | Padding::Optional => match self.group.len() {
                    1 => {
                        return Err(self.fail(Error::Truncated {
                            offset: self.group[0].1,
                        }));
                    }
                    2 => {
                        let first =
                            alphabet_value(self.group[0].0, self.options.alphabet).unwrap();
                        let second =
                            alphabet_value(self.group[1].0, self.options.alphabet).unwrap();
                        if self.options.strict && (second & 0x0f) != 0 {
                            return Err(self.fail(Error::NonCanonical {
                                offset: self.group[1].1,
                            }));
                        }
                        output.push((first << 2) | (second >> 4));
                    }
                    3 => {
                        let first =
                            alphabet_value(self.group[0].0, self.options.alphabet).unwrap();
                        let second =
                            alphabet_value(self.group[1].0, self.options.alphabet).unwrap();
                        let third =
                            alphabet_value(self.group[2].0, self.options.alphabet).unwrap();
                        if self.options.strict && (third & 0x03) != 0 {
                            return Err(self.fail(Error::NonCanonical {
                                offset: self.group[2].1,
                            }));
                        }
                        output.push((first << 2) | (second >> 4));
                        output.push((second << 4) | (third >> 2));
                    }
                    _ => {}
                },
            }
        }

        self.group.clear();
        self.closed = true;
        Ok(output)
    }
}