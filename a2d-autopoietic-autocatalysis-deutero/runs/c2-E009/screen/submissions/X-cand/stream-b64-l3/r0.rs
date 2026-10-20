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

fn digit_byte(value: u8, alphabet: Alphabet) -> u8 {
    match value {
        0..=25 => b'A' + value,
        26..=51 => b'a' + (value - 26),
        52..=61 => b'0' + (value - 52),
        62 if alphabet == Alphabet::Standard => b'+',
        63 if alphabet == Alphabet::Standard => b'/',
        62 => b'-',
        _ => b'_',
    }
}

fn emit_symbol(out: &mut String, byte: u8, whitespace: Whitespace, column: &mut usize) {
    if let Whitespace::Wrapped { width, ending } = whitespace {
        if *column == width {
            match ending {
                LineEnding::Lf => out.push('\n'),
                LineEnding::CrLf => out.push_str("\r\n"),
            }
            *column = 0;
        }
        *column += 1;
    }
    out.push(byte as char);
}

fn encode_group(
    bytes: &[u8],
    options: Options,
    out: &mut String,
    column: &mut usize,
) {
    let b0 = bytes[0];
    let b1 = if bytes.len() > 1 { bytes[1] } else { 0 };
    let b2 = if bytes.len() > 2 { bytes[2] } else { 0 };
    let values = [
        b0 >> 2,
        ((b0 & 0x03) << 4) | (b1 >> 4),
        ((b1 & 0x0f) << 2) | (b2 >> 6),
        b2 & 0x3f,
    ];
    let count = match bytes.len() {
        1 => 2,
        2 => 3,
        _ => 4,
    };
    for value in values.iter().take(count) {
        emit_symbol(out, digit_byte(*value, options.alphabet), options.whitespace, column);
    }
    if bytes.len() < 3 && options.padding != Padding::Forbidden {
        for _ in count..4 {
            emit_symbol(out, b'=', options.whitespace, column);
        }
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

pub struct Encoder {
    options: Options,
    tail: [u8; 2],
    tail_len: usize,
    finished: bool,
}

impl Encoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate_options(options)?;
        Ok(Self {
            options,
            tail: [0; 2],
            tail_len: 0,
            finished: false,
        })
    }

    pub fn push(&mut self, input: &[u8]) -> Result<String, Error> {
        if self.finished {
            return Err(Error::AlreadyFinished);
        }
        let mut out = String::new();
        let mut column = 0;
        let mut i = 0;

        if self.tail_len > 0 {
            while self.tail_len < 3 && i < input.len() {
                if self.tail_len < 2 {
                    self.tail[self.tail_len] = input[i];
                    self.tail_len += 1;
                } else {
                    let group = [self.tail[0], self.tail[1], input[i]];
                    encode_group(&group, self.options, &mut out, &mut column);
                    self.tail_len = 0;
                }
                i += 1;
            }
        }

        while i + 3 <= input.len() {
            encode_group(&input[i..i + 3], self.options, &mut out, &mut column);
            i += 3;
        }
        while i < input.len() {
            self.tail[self.tail_len] = input[i];
            self.tail_len += 1;
            i += 1;
        }
        Ok(out)
    }

    pub fn finish(&mut self) -> Result<String, Error> {
        if self.finished {
            return Err(Error::AlreadyFinished);
        }
        self.finished = true;
        let mut out = String::new();
        let mut column = 0;
        if self.tail_len > 0 {
            encode_group(
                &self.tail[..self.tail_len],
                self.options,
                &mut out,
                &mut column,
            );
            self.tail_len = 0;
        }
        Ok(out)
    }
}

#[derive(Clone, Copy)]
enum FrameState {
    Normal,
    NeedLf { cr_offset: usize },
    NeedSymbol { separator_offset: usize },
}

pub struct Decoder {
    options: Options,
    group: [u8; 4],
    offsets: [usize; 4],
    group_len: usize,
    pending_second_pad: Option<usize>,
    padded: bool,
    total_offset: usize,
    row_symbols: usize,
    frame_state: FrameState,
    sticky: Option<Error>,
    finished: bool,
}

impl Decoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate_options(options)?;
        Ok(Self {
            options,
            group: [0; 4],
            offsets: [0; 4],
            group_len: 0,
            pending_second_pad: None,
            padded: false,
            total_offset: 0,
            row_symbols: 0,
            frame_state: FrameState::Normal,
            sticky: None,
            finished: false,
        })
    }

    fn fail(&mut self, error: Error) -> Result<Vec<u8>, Error> {
        self.sticky = Some(error.clone());
        Err(error)
    }

    fn process_symbol(&mut self, byte: u8, offset: usize, out: &mut Vec<u8>) -> Result<(), Error> {
        if self.padded {
            return Err(Error::BadPadding { offset });
        }
        if self.pending_second_pad.is_some() && byte != b'=' {
            return Err(Error::BadPadding { offset });
        }
        if self.group_len < 2 && byte == b'=' {
            return Err(Error::BadPadding { offset });
        }

        if byte == b'=' {
            if self.options.padding == Padding::Forbidden {
                return Err(Error::BadPadding { offset });
            }
            if self.group_len == 2 {
                self.group[self.group_len] = byte;
                self.offsets[self.group_len] = offset;
                self.group_len += 1;
                self.pending_second_pad = Some(offset);
                return Ok(());
            }
            if self.group_len == 3 {
                self.group[self.group_len] = byte;
                self.offsets[self.group_len] = offset;
                self.group_len += 1;
                self.padded = true;
                self.emit_group(out)?;
                return Ok(());
            }
            return Err(Error::BadPadding { offset });
        }

        let value = alphabet_digit(byte, self.options.alphabet)
            .ok_or(Error::InvalidByte { offset, byte })?;
        self.group[self.group_len] = value;
        self.offsets[self.group_len] = offset;
        self.group_len += 1;

        if self.group_len == 4 {
            self.emit_group(out)?;
        }
        Ok(())
    }

    fn emit_group(&mut self, out: &mut Vec<u8>) -> Result<(), Error> {
        if self.group_len != 4 {
            return Ok(());
        }
        if self.group[0] > 63 || self.group[1] > 63 {
            return Err(Error::BadPadding { offset: self.offsets[2] });
        }
        let a = self.group[0];
        let b = self.group[1];
        out.push((a << 2) | (b >> 4));

        if self.group[2] != b'=' {
            let c = self.group[2];
            out.push((b << 4) | (c >> 2));
            if self.group[3] != b'=' {
                let d = self.group[3];
                out.push((c << 6) | d);
            }
        }
        self.group_len = 0;
        self.pending_second_pad = None;
        Ok(())
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
            let offset = self.total_offset;
            self.total_offset = self.total_offset.saturating_add(1);

            match self.options.whitespace {
                Whitespace::IgnoreAscii
                    if matches!(byte, 9 | 10 | 11 | 12 | 13 | 32) =>
                {
                    continue;
                }
                Whitespace::Wrapped { width, ending } => {
                    match self.frame_state {
                        FrameState::NeedLf { cr_offset } => {
                            if byte != b'\n' {
                                return self.fail(Error::BadWrap { offset });
                            }
                            self.frame_state = FrameState::NeedSymbol {
                                separator_offset: cr_offset,
                            };
                            continue;
                        }
                        FrameState::NeedSymbol { separator_offset } => {
                            self.frame_state = FrameState::Normal;
                            self.row_symbols = 0;
                            if matches!(byte, b'\r' | b'\n') {
                                return self.fail(Error::BadWrap { offset });
                            }
                            if let Err(error) = self.process_symbol(byte, offset, &mut out) {
                                return self.fail(error);
                            }
                            self.row_symbols += 1;
                            continue;
                        }
                        FrameState::Normal => {}
                    }

                    if self.row_symbols == width {
                        match ending {
                            LineEnding::Lf if byte == b'\n' => {
                                self.frame_state = FrameState::NeedSymbol {
                                    separator_offset: offset,
                                };
                                continue;
                            }
                            LineEnding::CrLf if byte == b'\r' => {
                                self.frame_state = FrameState::NeedLf { cr_offset: offset };
                                continue;
                            }
                            _ => return self.fail(Error::BadWrap { offset }),
                        }
                    } else if matches!(byte, b'\r' | b'\n') {
                        return self.fail(Error::BadWrap { offset });
                    }
                }
                _ => {}
            }

            if let Err(error) = self.process_symbol(byte, offset, &mut out) {
                return self.fail(error);
            }
            if matches!(self.options.whitespace, Whitespace::Wrapped { .. }) {
                self.row_symbols += 1;
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

        if let FrameState::NeedLf { cr_offset } = self.frame_state {
            return self.fail(Error::BadWrap { offset: cr_offset });
        }
        if let FrameState::NeedSymbol { separator_offset } = self.frame_state {
            return self.fail(Error::BadWrap { offset: separator_offset });
        }

        if let Some(offset) = self.pending_second_pad {
            return self.fail(Error::BadPadding { offset });
        }

        let mut out = Vec::new();
        if self.group_len > 0 {
            match self.options.padding {
                Padding::Required => {
                    return self.fail(Error::Truncated {
                        offset: self.offsets[0],
                    });
                }
                Padding::Forbidden | Padding::Optional => {
                    if self.group_len == 1 {
                        return self.fail(Error::Truncated {
                            offset: self.offsets[0],
                        });
                    }
                    let a = self.group[0];
                    let b = self.group[1];
                    out.push((a << 2) | (b >> 4));
                    if self.group_len == 3 {
                        let c = self.group[2];
                        out.push((b << 4) | (c >> 2));
                    }
                }
            }
        }

        self.finished = true;
        Ok(out)
    }
}