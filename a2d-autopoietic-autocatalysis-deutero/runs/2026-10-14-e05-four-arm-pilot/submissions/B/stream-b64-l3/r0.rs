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

fn alphabet_bytes(alphabet: Alphabet) -> &'static [u8; 64] {
    match alphabet {
        Alphabet::Standard => b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/",
        Alphabet::UrlSafe => b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_",
    }
}

fn digit(byte: u8, alphabet: Alphabet) -> Option<u8> {
    alphabet_bytes(alphabet)
        .iter()
        .position(|&b| b == byte)
        .map(|n| n as u8)
}

pub struct Encoder {
    options: Options,
    pending: Vec<u8>,
    finished: bool,
}

impl Encoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate(options)?;
        Ok(Self {
            options,
            pending: Vec::new(),
            finished: false,
        })
    }

    pub fn push(&mut self, input: &[u8]) -> Result<String, Error> {
        if self.finished {
            return Err(Error::AlreadyFinished);
        }
        self.pending.extend_from_slice(input);
        let complete = self.pending.len() / 3 * 3;
        let bytes: Vec<u8> = self.pending.drain(..complete).collect();
        Ok(render_symbols(&encode_full(&bytes, self.options.alphabet), self.options.whitespace))
    }

    pub fn finish(&mut self) -> Result<String, Error> {
        if self.finished {
            return Err(Error::AlreadyFinished);
        }
        self.finished = true;
        let tail = std::mem::take(&mut self.pending);
        let mut symbols = encode_full(&tail, self.options.alphabet);
        if !tail.is_empty() && self.options.padding != Padding::Forbidden {
            if tail.len() == 1 {
                symbols.push(b'=');
                symbols.push(b'=');
            } else {
                symbols.push(b'=');
            }
        }
        Ok(render_symbols(&symbols, self.options.whitespace))
    }
}

fn encode_full(input: &[u8], alphabet: Alphabet) -> Vec<u8> {
    let table = alphabet_bytes(alphabet);
    let mut out = Vec::with_capacity((input.len() + 2) / 3 * 4);
    for chunk in input.chunks(3) {
        if chunk.len() < 3 {
            let a = chunk[0];
            let b = if chunk.len() > 1 { chunk[1] } else { 0 };
            out.push(table[(a >> 2) as usize]);
            out.push(table[(((a & 0x03) << 4) | (b >> 4)) as usize]);
            if chunk.len() > 1 {
                out.push(table[((b & 0x0f) << 2) as usize]);
            }
        } else {
            let a = chunk[0];
            let b = chunk[1];
            let c = chunk[2];
            out.push(table[(a >> 2) as usize]);
            out.push(table[(((a & 0x03) << 4) | (b >> 4)) as usize]);
            out.push(table[(((b & 0x0f) << 2) | (c >> 6)) as usize]);
            out.push(table[(c & 0x3f) as usize]);
        }
    }
    out
}

fn render_symbols(symbols: &[u8], whitespace: Whitespace) -> String {
    match whitespace {
        Whitespace::None | Whitespace::IgnoreAscii => {
            String::from_utf8(symbols.to_vec()).expect("base64 symbols are ASCII")
        }
        Whitespace::Wrapped { width, ending } => {
            let separator: &[u8] = match ending {
                LineEnding::Lf => b"\n",
                LineEnding::CrLf => b"\r\n",
            };
            let mut out = Vec::with_capacity(symbols.len() + symbols.len() / width * separator.len());
            for (i, chunk) in symbols.chunks(width).enumerate() {
                if i > 0 {
                    out.extend_from_slice(separator);
                }
                out.extend_from_slice(chunk);
            }
            String::from_utf8(out).expect("base64 symbols are ASCII")
        }
    }
}

pub struct Decoder {
    options: Options,
    offset: usize,
    group: Vec<(u8, usize)>,
    pending_second_pad: Option<usize>,
    terminal: bool,
    sticky: Option<Error>,
    finished: bool,
    row_symbols: usize,
    wrap_state: WrapState,
}

#[derive(Clone, Copy)]
enum WrapState {
    Normal,
    NeedLf { cr_offset: usize },
    NeedSymbol { separator_offset: usize },
}

impl Decoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate(options)?;
        Ok(Self {
            options,
            offset: 0,
            group: Vec::with_capacity(4),
            pending_second_pad: None,
            terminal: false,
            sticky: None,
            finished: false,
            row_symbols: 0,
            wrap_state: WrapState::Normal,
        })
    }

    pub fn push(&mut self, input: &[u8]) -> Result<Vec<u8>, Error> {
        if let Some(error) = &self.sticky {
            return Err(error.clone());
        }
        if self.finished {
            return Err(Error::AlreadyFinished);
        }

        let mut output = Vec::new();
        for &byte in input {
            let offset = self.offset;
            self.offset += 1;
            if let Err(error) = self.process_byte(byte, offset, &mut output) {
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
                self.sticky = Some(error.clone());
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
                self.process_wrapped(byte, offset, width, ending, output)
            }
        }
    }

    fn process_wrapped(
        &mut self,
        byte: u8,
        offset: usize,
        width: usize,
        ending: LineEnding,
        output: &mut Vec<u8>,
    ) -> Result<(), Error> {
        match self.wrap_state {
            WrapState::NeedLf { cr_offset } => {
                if byte != b'\n' {
                    return Err(Error::BadWrap { offset });
                }
                self.wrap_state = WrapState::NeedSymbol {
                    separator_offset: cr_offset,
                };
                Ok(())
            }
            WrapState::NeedSymbol { .. } => {
                if is_ascii_whitespace(byte) {
                    return Err(Error::BadWrap { offset });
                }
                self.wrap_state = WrapState::Normal;
                self.row_symbols = 0;
                self.process_symbol(byte, offset, output)
            }
            WrapState::Normal => {
                if self.row_symbols == width {
                    match ending {
                        LineEnding::Lf if byte == b'\n' => {
                            self.wrap_state = WrapState::NeedSymbol {
                                separator_offset: offset,
                            };
                            Ok(())
                        }
                        LineEnding::CrLf if byte == b'\r' => {
                            self.wrap_state = WrapState::NeedLf { cr_offset: offset };
                            Ok(())
                        }
                        _ => Err(Error::BadWrap { offset }),
                    }
                } else if is_ascii_whitespace(byte) {
                    Err(Error::BadWrap { offset })
                } else {
                    self.row_symbols += 1;
                    self.process_symbol(byte, offset, output)
                }
            }
        }
    }

    fn process_symbol(
        &mut self,
        byte: u8,
        offset: usize,
        output: &mut Vec<u8>,
    ) -> Result<(), Error> {
        if self.terminal {
            return Err(Error::BadPadding { offset });
        }
        if let Some(pad_offset) = self.pending_second_pad {
            if byte != b'=' {
                return Err(Error::BadPadding { offset });
            }
            self.pending_second_pad = None;
            self.group.push((byte, offset));
            self.complete_group(output)?;
            self.terminal = true;
            let _ = pad_offset;
            return Ok(());
        }

        let position = self.group.len();
        if byte == b'=' {
            if position <= 1 {
                return Err(Error::BadPadding { offset });
            }
            if position == 2 {
                if self.options.padding == Padding::Forbidden {
                    return Err(Error::BadPadding { offset });
                }
                self.pending_second_pad = Some(offset);
                self.group.push((byte, offset));
                return Ok(());
            }
            if position == 3 {
                if self.options.padding == Padding::Forbidden {
                    return Err(Error::BadPadding { offset });
                }
                self.group.push((byte, offset));
                self.complete_group(output)?;
                self.terminal = true;
                return Ok(());
            }
        }

        let value = digit(byte, self.options.alphabet)
            .ok_or(Error::InvalidByte { offset, byte })?;
        self.group.push((byte, offset));
        if self.group.len() == 4 {
            self.complete_group(output)?;
        }
        let _ = value;
        Ok(())
    }

    fn complete_group(&mut self, output: &mut Vec<u8>) -> Result<(), Error> {
        let g = &self.group;
        if g.len() != 4 {
            return Ok(());
        }
        let a = digit(g[0].0, self.options.alphabet)
            .ok_or(Error::InvalidByte { offset: g[0].1, byte: g[0].0 })?;
        let b = digit(g[1].0, self.options.alphabet)
            .ok_or(Error::InvalidByte { offset: g[1].1, byte: g[1].0 })?;
        output.push((a << 2) | (b >> 4));

        if g[2].0 == b'=' {
            self.group.clear();
            return Ok(());
        }
        let c = digit(g[2].0, self.options.alphabet)
            .ok_or(Error::InvalidByte { offset: g[2].1, byte: g[2].0 })?;
        output.push((b << 4) | (c >> 2));

        if g[3].0 == b'=' {
            self.group.clear();
            return Ok(());
        }
        let d = digit(g[3].0, self.options.alphabet)
            .ok_or(Error::InvalidByte { offset: g[3].1, byte: g[3].0 })?;
        output.push((c << 6) | d);
        self.group.clear();
        Ok(())
    }

    fn finish_inner(&mut self) -> Result<Vec<u8>, Error> {
        if let WrapState::NeedLf { cr_offset } = self.wrap_state {
            return Err(Error::BadWrap { offset: cr_offset });
        }
        if let WrapState::NeedSymbol { separator_offset } = self.wrap_state {
            return Err(Error::BadWrap {
                offset: separator_offset,
            });
        }
        if let Whitespace::Wrapped { width, .. } = self.options.whitespace {
            if self.row_symbols > width {
                return Err(Error::BadWrap { offset: self.offset });
            }
        }

        if let Some(offset) = self.pending_second_pad {
            return Err(Error::BadPadding { offset });
        }

        let mut output = Vec::new();
        if self.group.is_empty() {
            return Ok(output);
        }

        if self.options.padding == Padding::Required {
            return Err(Error::Truncated {
                offset: self.group[0].1,
            });
        }

        match self.group.len() {
            1 => Err(Error::Truncated {
                offset: self.group[0].1,
            }),
            2 => {
                let a = digit(self.group[0].0, self.options.alphabet)
                    .ok_or(Error::InvalidByte {
                        offset: self.group[0].1,
                        byte: self.group[0].0,
                    })?;
                let b = digit(self.group[1].0, self.options.alphabet)
                    .ok_or(Error::InvalidByte {
                        offset: self.group[1].1,
                        byte: self.group[1].0,
                    })?;
                output.push((a << 2) | (b >> 4));
                Ok(output)
            }
            3 => {
                let a = digit(self.group[0].0, self.options.alphabet)
                    .ok_or(Error::InvalidByte {
                        offset: self.group[0].1,
                        byte: self.group[0].0,
                    })?;
                let b = digit(self.group[1].0, self.options.alphabet)
                    .ok_or(Error::InvalidByte {
                        offset: self.group[1].1,
                        byte: self.group[1].0,
                    })?;
                let c = digit(self.group[2].0, self.options.alphabet)
                    .ok_or(Error::InvalidByte {
                        offset: self.group[2].1,
                        byte: self.group[2].0,
                    })?;
                output.push((a << 2) | (b >> 4));
                output.push((b << 4) | (c >> 2));
                Ok(output)
            }
            _ => Ok(output),
        }
    }
}

fn is_ascii_whitespace(byte: u8) -> bool {
    matches!(byte, 9 | 10 | 11 | 12 | 13 | 32)
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
    output.extend_from_slice(&decoder.finish()?);
    Ok(output)
}