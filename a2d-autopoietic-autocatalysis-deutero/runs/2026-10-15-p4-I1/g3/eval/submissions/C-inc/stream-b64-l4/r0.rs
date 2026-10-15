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

fn alphabet_symbol(alphabet: Alphabet, value: u8) -> u8 {
    match (alphabet, value) {
        (_, 0..=25) => b'A' + value,
        (_, 26..=51) => b'a' + value - 26,
        (_, 52..=61) => b'0' + value - 52,
        (Alphabet::Standard, 62) => b'+',
        (Alphabet::Standard, 63) => b'/',
        (Alphabet::UrlSafe, 62) => b'-',
        (Alphabet::UrlSafe, 63) => b'_',
        _ => unreachable!(),
    }
}

fn ignored_ascii(byte: u8) -> bool {
    matches!(byte, 9 | 10 | 11 | 12 | 13 | 32)
}

fn wrap_parts(whitespace: Whitespace) -> Option<(usize, LineEnding)> {
    match whitespace {
        Whitespace::Wrapped { width, ending } => Some((width, ending)),
        _ => None,
    }
}

pub struct Encoder {
    options: Options,
    pending: Vec<u8>,
    symbols_in_row: usize,
    closed: bool,
}

impl Encoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate(options)?;
        Ok(Self {
            options,
            pending: Vec::new(),
            symbols_in_row: 0,
            closed: false,
        })
    }

    pub fn push(&mut self, input: &[u8]) -> Result<String, Error> {
        if self.closed {
            return Err(Error::AlreadyFinished);
        }

        self.pending.extend_from_slice(input);
        let complete_len = self.pending.len() / 3 * 3;
        let complete: Vec<u8> = self.pending.drain(..complete_len).collect();
        Ok(self.encode_complete(&complete))
    }

    pub fn finish(&mut self) -> Result<String, Error> {
        if self.closed {
            return Err(Error::AlreadyFinished);
        }
        self.closed = true;

        let tail = std::mem::take(&mut self.pending);
        if tail.is_empty() {
            return Ok(String::new());
        }

        let b0 = tail[0];
        let b1 = *tail.get(1).unwrap_or(&0);
        let values = [
            b0 >> 2,
            ((b0 & 0x03) << 4) | (b1 >> 4),
            (b1 & 0x0f) << 2,
        ];
        let digit_count = if tail.len() == 1 { 2 } else { 3 };
        let mut out = String::new();

        for &value in &values[..digit_count] {
            self.emit_symbol(&mut out, alphabet_symbol(self.options.alphabet, value));
        }
        if self.options.padding != Padding::Forbidden {
            for _ in digit_count..4 {
                self.emit_symbol(&mut out, b'=');
            }
        }
        Ok(out)
    }

    fn encode_complete(&mut self, bytes: &[u8]) -> String {
        let mut out = String::new();
        for chunk in bytes.chunks_exact(3) {
            let b0 = chunk[0];
            let b1 = chunk[1];
            let b2 = chunk[2];
            let values = [
                b0 >> 2,
                ((b0 & 0x03) << 4) | (b1 >> 4),
                ((b1 & 0x0f) << 2) | (b2 >> 6),
                b2 & 0x3f,
            ];
            for value in values {
                self.emit_symbol(&mut out, alphabet_symbol(self.options.alphabet, value));
            }
        }
        out
    }

    fn emit_symbol(&mut self, out: &mut String, byte: u8) {
        if let Some((width, ending)) = wrap_parts(self.options.whitespace) {
            if self.symbols_in_row == width {
                match ending {
                    LineEnding::Lf => out.push('\n'),
                    LineEnding::CrLf => out.push_str("\r\n"),
                }
                self.symbols_in_row = 0;
            }
            self.symbols_in_row += 1;
        }
        out.push(byte as char);
    }
}

#[derive(Clone, Copy)]
enum FramingState {
    Normal,
    NeedLf { cr_offset: usize },
}

pub struct Decoder {
    options: Options,
    offset: usize,
    symbol_count: usize,
    group: [u8; 4],
    group_offsets: [usize; 4],
    group_len: usize,
    pending_second_pad: Option<usize>,
    padded_done: bool,
    row_symbols: usize,
    framing_state: FramingState,
    after_separator: Option<usize>,
    closed: bool,
    sticky: Option<Error>,
}

impl Decoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate(options)?;
        Ok(Self {
            options,
            offset: 0,
            symbol_count: 0,
            group: [0; 4],
            group_offsets: [0; 4],
            group_len: 0,
            pending_second_pad: None,
            padded_done: false,
            row_symbols: 0,
            framing_state: FramingState::Normal,
            after_separator: None,
            closed: false,
            sticky: None,
        })
    }

    pub fn push(&mut self, input: &[u8]) -> Result<Vec<u8>, Error> {
        if let Some(error) = &self.sticky {
            return Err(error.clone());
        }
        if self.closed {
            return Err(Error::AlreadyFinished);
        }

        let mut out = Vec::new();
        for &byte in input {
            let offset = self.offset;
            self.offset += 1;
            if let Err(error) = self.process_byte(byte, offset, &mut out) {
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
        if self.closed {
            return Err(Error::AlreadyFinished);
        }

        let mut out = Vec::new();
        if let Err(error) = self.finish_inner(&mut out) {
            self.sticky = Some(error.clone());
            return Err(error);
        }
        self.closed = true;
        Ok(out)
    }

    fn process_byte(
        &mut self,
        byte: u8,
        offset: usize,
        out: &mut Vec<u8>,
    ) -> Result<(), Error> {
        if self.options.whitespace == Whitespace::IgnoreAscii && ignored_ascii(byte) {
            return Ok(());
        }

        if let Some((width, ending)) = wrap_parts(self.options.whitespace) {
            if let FramingState::NeedLf { cr_offset } = self.framing_state {
                if byte != b'\n' {
                    return Err(Error::BadWrap { offset });
                }
                self.framing_state = FramingState::Normal;
                self.after_separator = Some(cr_offset);
                return Ok(());
            }

            if self.row_symbols == width {
                match ending {
                    LineEnding::Lf => {
                        if byte != b'\n' {
                            return Err(Error::BadWrap { offset });
                        }
                        self.row_symbols = 0;
                        self.after_separator = Some(offset);
                        return Ok(());
                    }
                    LineEnding::CrLf => {
                        if byte != b'\r' {
                            return Err(Error::BadWrap { offset });
                        }
                        self.framing_state = FramingState::NeedLf { cr_offset: offset };
                        return Ok(());
                    }
                }
            }

            if ignored_ascii(byte) {
                return Err(Error::BadWrap { offset });
            }

            if self.after_separator.is_some() {
                self.after_separator = None;
            }
        }

        if let Some(limit) = self.options.max_symbols {
            if self.symbol_count >= limit {
                return Err(Error::Overlong { offset, limit });
            }
        }
        self.symbol_count += 1;

        if self.padded_done {
            return Err(Error::BadPadding { offset });
        }

        if let Some(pad_offset) = self.pending_second_pad {
            if byte != b'=' {
                return Err(Error::BadPadding { offset });
            }
            self.pending_second_pad = None;
            self.group[2] = b'=';
            self.group_offsets[2] = pad_offset;
            self.group[3] = b'=';
            self.group_offsets[3] = offset;
            self.group_len = 4;
            self.complete_group(out)?;
            return Ok(());
        }

        if byte == b'=' {
            match self.group_len {
                0 | 1 => return Err(Error::BadPadding { offset }),
                2 if self.options.padding == Padding::Forbidden => {
                    return Err(Error::BadPadding { offset });
                }
                2 => {
                    self.pending_second_pad = Some(offset);
                    return Ok(());
                }
                3 if self.options.padding == Padding::Forbidden => {
                    return Err(Error::BadPadding { offset });
                }
                3 => {
                    self.group[3] = b'=';
                    self.group_offsets[3] = offset;
                    self.group_len = 4;
                    self.complete_group(out)?;
                    return Ok(());
                }
                _ => unreachable!(),
            }
        }

        if let Some(value) = self.decode_digit(byte) {
            self.group[self.group_len] = value;
            self.group_offsets[self.group_len] = offset;
            self.group_len += 1;

            if matches!(self.options.whitespace, Whitespace::Wrapped { .. }) {
                self.row_symbols += 1;
            }

            if self.group_len == 4 {
                self.complete_group(out)?;
            }
            Ok(())
        } else if self.options.strict && self.is_foreign(byte) {
            Err(Error::MixedAlphabet { offset })
        } else {
            Err(Error::InvalidByte { offset, byte })
        }
    }

    fn decode_digit(&self, byte: u8) -> Option<u8> {
        match byte {
            b'A'..=b'Z' => Some(byte - b'A'),
            b'a'..=b'z' => Some(byte - b'a' + 26),
            b'0'..=b'9' => Some(byte - b'0' + 52),
            b'+' if self.options.alphabet == Alphabet::Standard => Some(62),
            b'/' if self.options.alphabet == Alphabet::Standard => Some(63),
            b'-' if self.options.alphabet == Alphabet::UrlSafe => Some(62),
            b'_' if self.options.alphabet == Alphabet::UrlSafe => Some(63),
            _ => None,
        }
    }

    fn is_foreign(&self, byte: u8) -> bool {
        match self.options.alphabet {
            Alphabet::Standard => matches!(byte, b'-' | b'_'),
            Alphabet::UrlSafe => matches!(byte, b'+' | b'/'),
        }
    }

    fn complete_group(&mut self, out: &mut Vec<u8>) -> Result<(), Error> {
        let va = self.group[0];
        let vb = self.group[1];

        if self.group[2] == b'=' {
            if self.group[3] != b'=' {
                return Err(Error::BadPadding {
                    offset: self.group_offsets[3],
                });
            }
            if self.options.strict && vb & 0x0f != 0 {
                return Err(Error::NonCanonical {
                    offset: self.group_offsets[1],
                });
            }
            out.push((va << 2) | (vb >> 4));
            self.padded_done = true;
        } else if self.group[3] == b'=' {
            let vc = self.group[2];
            if self.options.strict && vc & 0x03 != 0 {
                return Err(Error::NonCanonical {
                    offset: self.group_offsets[2],
                });
            }
            out.push((va << 2) | (vb >> 4));
            out.push((vb << 4) | (vc >> 2));
            self.padded_done = true;
        } else {
            let vc = self.group[2];
            let vd = self.group[3];
            out.push((va << 2) | (vb >> 4));
            out.push((vb << 4) | (vc >> 2));
            out.push((vc << 6) | vd);
        }

        self.group_len = 0;
        Ok(())
    }

    fn finish_inner(&mut self, out: &mut Vec<u8>) -> Result<(), Error> {
        if let FramingState::NeedLf { cr_offset } = self.framing_state {
            return Err(Error::BadWrap { offset: cr_offset });
        }
        if let Some(separator_offset) = self.after_separator {
            return Err(Error::BadWrap {
                offset: separator_offset,
            });
        }
        if let Some(pad_offset) = self.pending_second_pad {
            return Err(Error::BadPadding { offset: pad_offset });
        }
        if self.group_len == 0 {
            return Ok(());
        }

        if self.options.padding == Padding::Required {
            return Err(Error::Truncated {
                offset: self.group_offsets[0],
            });
        }

        match self.group_len {
            1 => Err(Error::Truncated {
                offset: self.group_offsets[0],
            }),
            2 => {
                if self.options.strict && self.group[1] & 0x0f != 0 {
                    return Err(Error::NonCanonical {
                        offset: self.group_offsets[1],
                    });
                }
                out.push((self.group[0] << 2) | (self.group[1] >> 4));
                Ok(())
            }
            3 => {
                if self.options.strict && self.group[2] & 0x03 != 0 {
                    return Err(Error::NonCanonical {
                        offset: self.group_offsets[2],
                    });
                }
                out.push((self.group[0] << 2) | (self.group[1] >> 4));
                out.push((self.group[1] << 4) | (self.group[2] >> 2));
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

pub fn encode(input: &[u8], options: Options) -> Result<String, Error> {
    validate(options)?;
    let mut encoder = Encoder::new(options)?;
    let mut result = encoder.push(input)?;
    result.push_str(&encoder.finish()?);
    Ok(result)
}

pub fn decode(input: &[u8], options: Options) -> Result<Vec<u8>, Error> {
    validate(options)?;
    let mut decoder = Decoder::new(options)?;
    let mut result = decoder.push(input)?;
    result.extend(decoder.finish()?);
    Ok(result)
}

fn main() {}