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

fn alphabet_bytes(alphabet: Alphabet) -> &'static [u8; 64] {
    match alphabet {
        Alphabet::Standard => b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/",
        Alphabet::UrlSafe => b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_",
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

pub struct Encoder {
    options: Options,
    tail: [u8; 2],
    tail_len: usize,
    column: usize,
    closed: bool,
}

impl Encoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate(options)?;
        Ok(Self {
            options,
            tail: [0; 2],
            tail_len: 0,
            column: 0,
            closed: false,
        })
    }

    fn emit_symbol(&mut self, output: &mut String, symbol: u8) {
        if let Whitespace::Wrapped { width, ending } = self.options.whitespace {
            if self.column == width {
                match ending {
                    LineEnding::Lf => output.push('\n'),
                    LineEnding::CrLf => output.push_str("\r\n"),
                }
                self.column = 0;
            }
        }
        output.push(symbol as char);
        self.column += 1;
    }

    fn emit_group(&mut self, output: &mut String, a: u8, b: u8, c: u8) {
        let alphabet = alphabet_bytes(self.options.alphabet);
        self.emit_symbol(output, alphabet[(a >> 2) as usize]);
        self.emit_symbol(output, alphabet[(((a & 3) << 4) | (b >> 4)) as usize]);
        self.emit_symbol(output, alphabet[(((b & 15) << 2) | (c >> 6)) as usize]);
        self.emit_symbol(output, alphabet[(c & 63) as usize]);
    }

    pub fn push(&mut self, input: &[u8]) -> Result<String, Error> {
        if self.closed {
            return Err(Error::AlreadyFinished);
        }

        let mut output = String::new();
        let mut index = 0;

        if self.tail_len > 0 {
            while self.tail_len < 2 && index < input.len() {
                self.tail[self.tail_len] = input[index];
                self.tail_len += 1;
                index += 1;
            }
            if self.tail_len == 2 && index < input.len() {
                self.emit_group(&mut output, self.tail[0], self.tail[1], input[index]);
                self.tail_len = 0;
                index += 1;
            } else if self.tail_len < 2 || index == input.len() {
                return Ok(output);
            }
        }

        while index + 3 <= input.len() {
            self.emit_group(
                &mut output,
                input[index],
                input[index + 1],
                input[index + 2],
            );
            index += 3;
        }

        self.tail_len = input.len() - index;
        for i in 0..self.tail_len {
            self.tail[i] = input[index + i];
        }

        Ok(output)
    }

    pub fn finish(&mut self) -> Result<String, Error> {
        if self.closed {
            return Err(Error::AlreadyFinished);
        }
        self.closed = true;

        let mut output = String::new();
        if self.tail_len == 1 {
            let alphabet = alphabet_bytes(self.options.alphabet);
            let a = self.tail[0];
            self.emit_symbol(&mut output, alphabet[(a >> 2) as usize]);
            self.emit_symbol(&mut output, alphabet[((a & 3) << 4) as usize]);
            if self.options.padding != Padding::Forbidden {
                self.emit_symbol(&mut output, b'=');
                self.emit_symbol(&mut output, b'=');
            }
        } else if self.tail_len == 2 {
            let alphabet = alphabet_bytes(self.options.alphabet);
            let a = self.tail[0];
            let b = self.tail[1];
            self.emit_symbol(&mut output, alphabet[(a >> 2) as usize]);
            self.emit_symbol(&mut output, alphabet[(((a & 3) << 4) | (b >> 4)) as usize]);
            self.emit_symbol(&mut output, alphabet[((b & 15) << 2) as usize]);
            if self.options.padding != Padding::Forbidden {
                self.emit_symbol(&mut output, b'=');
            }
        }
        self.tail_len = 0;
        Ok(output)
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
    group_offsets: [usize; 4],
    group_len: usize,
    pending_second_pad: Option<usize>,
    terminal_padding: bool,
    offset: usize,
    column: usize,
    frame: FrameState,
    closed: bool,
    sticky_error: Option<Error>,
}

impl Decoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate(options)?;
        Ok(Self {
            options,
            group: [0; 4],
            group_offsets: [0; 4],
            group_len: 0,
            pending_second_pad: None,
            terminal_padding: false,
            offset: 0,
            column: 0,
            frame: FrameState::Normal,
            closed: false,
            sticky_error: None,
        })
    }

    fn fail(&mut self, error: Error) -> Error {
        self.sticky_error = Some(error.clone());
        error
    }

    fn decode_complete_group(&mut self, output: &mut Vec<u8>) -> Result<(), Error> {
        let a = digit_value(self.group[0], self.options.alphabet).unwrap();
        let b = digit_value(self.group[1], self.options.alphabet).unwrap();

        if self.group[2] == b'=' {
            if self.options.padding == Padding::Forbidden {
                return Err(Error::BadPadding {
                    offset: self.group_offsets[2],
                });
            }
            output.push((a << 2) | (b >> 4));
            self.terminal_padding = true;
        } else {
            let c = digit_value(self.group[2], self.options.alphabet).unwrap();
            if self.group[3] == b'=' {
                if self.options.padding == Padding::Forbidden {
                    return Err(Error::BadPadding {
                        offset: self.group_offsets[3],
                    });
                }
                output.push((a << 2) | (b >> 4));
                output.push((b << 4) | (c >> 2));
                self.terminal_padding = true;
            } else {
                let d = digit_value(self.group[3], self.options.alphabet).unwrap();
                output.push((a << 2) | (b >> 4));
                output.push((b << 4) | (c >> 2));
                output.push((c << 6) | d);
            }
        }

        self.group_len = 0;
        Ok(())
    }

    fn process_symbol(&mut self, byte: u8, offset: usize, output: &mut Vec<u8>) -> Result<(), Error> {
        if self.terminal_padding {
            return Err(Error::BadPadding { offset });
        }
        if self.pending_second_pad.is_some() {
            if byte != b'=' {
                return Err(Error::BadPadding { offset });
            }
            self.group[3] = b'=';
            self.group_offsets[3] = offset;
            self.pending_second_pad = None;
            self.decode_complete_group(output)?;
            return Ok(());
        }

        let position = self.group_len;
        if byte == b'=' {
            match position {
                0 | 1 => return Err(Error::BadPadding { offset }),
                2 => {
                    self.group[2] = b'=';
                    self.group_offsets[2] = offset;
                    self.group_len = 3;
                    self.pending_second_pad = Some(offset);
                    return Ok(());
                }
                3 => {
                    if self.options.padding == Padding::Forbidden {
                        return Err(Error::BadPadding { offset });
                    }
                    self.group[3] = b'=';
                    self.group_offsets[3] = offset;
                    self.decode_complete_group(output)?;
                    return Ok(());
                }
                _ => unreachable!(),
            }
        }

        let value = digit_value(byte, self.options.alphabet)
            .ok_or(Error::InvalidByte { offset, byte })?;
        self.group[position] = byte;
        self.group_offsets[position] = offset;
        self.group_len += 1;
        if self.group_len == 4 {
            let _ = value;
            self.decode_complete_group(output)?;
        }
        Ok(())
    }

    fn process_byte(&mut self, byte: u8, offset: usize, output: &mut Vec<u8>) -> Result<(), Error> {
        if let Whitespace::Wrapped { width, ending } = self.options.whitespace {
            match self.frame {
                FrameState::NeedLf { cr_offset } => {
                    if byte != b'\n' {
                        return Err(Error::BadWrap { offset });
                    }
                    self.frame = FrameState::NeedSymbol {
                        separator_offset: cr_offset,
                    };
                    return Ok(());
                }
                FrameState::NeedSymbol { .. } => {
                    self.frame = FrameState::Normal;
                }
                FrameState::Normal => {
                    if self.column == width {
                        match ending {
                            LineEnding::Lf if byte == b'\n' => {
                                self.frame = FrameState::NeedSymbol {
                                    separator_offset: offset,
                                };
                                self.column = 0;
                                return Ok(());
                            }
                            LineEnding::CrLf if byte == b'\r' => {
                                self.frame = FrameState::NeedLf { cr_offset: offset };
                                self.column = 0;
                                return Ok(());
                            }
                            _ => return Err(Error::BadWrap { offset }),
                        }
                    } else if byte == b'\r' || byte == b'\n' || byte == 9 || byte == 11 || byte == 12 || byte == 32 {
                        return Err(Error::BadWrap { offset });
                    }
                }
            }

            if let FrameState::NeedSymbol { separator_offset } = self.frame {
                if byte == b'\r' || byte == b'\n' || byte == 9 || byte == 11 || byte == 12 || byte == 32 {
                    return Err(Error::BadWrap { offset });
                }
                self.frame = FrameState::Normal;
                let _ = separator_offset;
            }

            if matches!(self.frame, FrameState::Normal) {
                self.column += 1;
            }
        } else if self.options.whitespace == Whitespace::IgnoreAscii
            && matches!(byte, 9 | 10 | 11 | 12 | 13 | 32)
        {
            return Ok(());
        }

        self.process_symbol(byte, offset, output)
    }

    pub fn push(&mut self, input: &[u8]) -> Result<Vec<u8>, Error> {
        if let Some(error) = &self.sticky_error {
            return Err(error.clone());
        }
        if self.closed {
            return Err(Error::AlreadyFinished);
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
            return Err(Error::AlreadyFinished);
        }

        if let FrameState::NeedLf { cr_offset } = self.frame {
            let error = self.fail(Error::BadWrap { offset: cr_offset });
            return Err(error);
        }
        if let FrameState::NeedSymbol { separator_offset } = self.frame {
            let error = self.fail(Error::BadWrap {
                offset: separator_offset,
            });
            return Err(error);
        }
        if let Some(offset) = self.pending_second_pad {
            let error = self.fail(Error::BadPadding { offset });
            return Err(error);
        }

        let mut output = Vec::new();
        if self.group_len == 0 {
            self.closed = true;
            return Ok(output);
        }

        if self.options.padding == Padding::Required {
            let error = self.fail(Error::Truncated {
                offset: self.group_offsets[0],
            });
            return Err(error);
        }

        match self.group_len {
            1 => {
                let error = self.fail(Error::Truncated {
                    offset: self.group_offsets[0],
                });
                Err(error)
            }
            2 => {
                let a = digit_value(self.group[0], self.options.alphabet).unwrap();
                let b = digit_value(self.group[1], self.options.alphabet).unwrap();
                output.push((a << 2) | (b >> 4));
                self.closed = true;
                Ok(output)
            }
            3 => {
                let a = digit_value(self.group[0], self.options.alphabet).unwrap();
                let b = digit_value(self.group[1], self.options.alphabet).unwrap();
                let c = digit_value(self.group[2], self.options.alphabet).unwrap();
                output.push((a << 2) | (b >> 4));
                output.push((b << 4) | (c >> 2));
                self.closed = true;
                Ok(output)
            }
            _ => unreachable!(),
        }
    }
}