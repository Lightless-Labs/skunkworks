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

fn digit(byte: u8, alphabet: Alphabet) -> Option<u8> {
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

fn encoded_digit(n: u8, alphabet: Alphabet) -> u8 {
    match n {
        0..=25 => b'A' + n,
        26..=51 => b'a' + (n - 26),
        52..=61 => b'0' + (n - 52),
        62 => if alphabet == Alphabet::Standard { b'+' } else { b'-' },
        _ => if alphabet == Alphabet::Standard { b'/' } else { b'_' },
    }
}

fn encode_raw(input: &[u8], options: Options) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while input.len().saturating_sub(i) >= 3 {
        let a = input[i];
        let b = input[i + 1];
        let c = input[i + 2];
        out.push(encoded_digit(a >> 2, options.alphabet));
        out.push(encoded_digit(((a & 3) << 4) | (b >> 4), options.alphabet));
        out.push(encoded_digit(((b & 15) << 2) | (c >> 6), options.alphabet));
        out.push(encoded_digit(c & 63, options.alphabet));
        i += 3;
    }
    match input.len() - i {
        1 => {
            let a = input[i];
            out.push(encoded_digit(a >> 2, options.alphabet));
            out.push(encoded_digit((a & 3) << 4, options.alphabet));
            if options.padding != Padding::Forbidden {
                out.extend_from_slice(b"==");
            }
        }
        2 => {
            let a = input[i];
            let b = input[i + 1];
            out.push(encoded_digit(a >> 2, options.alphabet));
            out.push(encoded_digit(((a & 3) << 4) | (b >> 4), options.alphabet));
            out.push(encoded_digit((b & 15) << 2, options.alphabet));
            if options.padding != Padding::Forbidden {
                out.push(b'=');
            }
        }
        _ => {}
    }
    out
}

fn wrap_symbols(raw: &[u8], width: usize, ending: LineEnding) -> Vec<u8> {
    let separator: &[u8] = match ending {
        LineEnding::Lf => b"\n",
        LineEnding::CrLf => b"\r\n",
    };
    let mut out = Vec::new();
    for (i, chunk) in raw.chunks(width).enumerate() {
        if i > 0 {
            out.extend_from_slice(separator);
        }
        out.extend_from_slice(chunk);
    }
    out
}

pub fn encode(input: &[u8], options: Options) -> Result<String, Error> {
    validate(options)?;
    let raw = encode_raw(input, options);
    let bytes = match options.whitespace {
        Whitespace::Wrapped { width, ending } => wrap_symbols(&raw, width, ending),
        _ => raw,
    };
    Ok(String::from_utf8(bytes).expect("base64 alphabet is ASCII"))
}

pub struct Encoder {
    options: Options,
    tail: Vec<u8>,
    closed: bool,
    emitted_symbols: usize,
}

impl Encoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate(options)?;
        Ok(Self {
            options,
            tail: Vec::new(),
            closed: false,
            emitted_symbols: 0,
        })
    }

    fn wrap_new(&mut self, raw: Vec<u8>) -> Vec<u8> {
        let mut out = Vec::new();
        if let Whitespace::Wrapped { width, ending } = self.options.whitespace {
            let separator: &[u8] = match ending {
                LineEnding::Lf => b"\n",
                LineEnding::CrLf => b"\r\n",
            };
            for byte in raw {
                if self.emitted_symbols > 0 && self.emitted_symbols % width == 0 {
                    out.extend_from_slice(separator);
                }
                out.push(byte);
                self.emitted_symbols += 1;
            }
        } else {
            out = raw;
        }
        out
    }

    pub fn push(&mut self, input: &[u8]) -> Result<String, Error> {
        if self.closed {
            return Err(Error::AlreadyFinished);
        }
        let mut data = Vec::with_capacity(self.tail.len().saturating_add(input.len()));
        data.extend_from_slice(&self.tail);
        data.extend_from_slice(input);
        let complete_len = data.len() / 3 * 3;
        let raw = encode_raw(&data[..complete_len], self.options);
        self.tail.clear();
        self.tail.extend_from_slice(&data[complete_len..]);
        Ok(String::from_utf8(self.wrap_new(raw)).expect("base64 alphabet is ASCII"))
    }

    pub fn finish(&mut self) -> Result<String, Error> {
        if self.closed {
            return Err(Error::AlreadyFinished);
        }
        self.closed = true;
        let raw = encode_raw(&self.tail, self.options);
        self.tail.clear();
        Ok(String::from_utf8(self.wrap_new(raw)).expect("base64 alphabet is ASCII"))
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FrameState {
    Normal,
    NeedLf(usize),
    NeedSymbol(usize),
}

pub struct Decoder {
    options: Options,
    group: Vec<(u8, usize)>,
    offset: usize,
    terminal: bool,
    sticky: Option<Error>,
    closed: bool,
    row_symbols: usize,
    frame: FrameState,
}

impl Decoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate(options)?;
        Ok(Self {
            options,
            group: Vec::new(),
            offset: 0,
            terminal: false,
            sticky: None,
            closed: false,
            row_symbols: 0,
            frame: FrameState::Normal,
        })
    }

    fn fail(&mut self, error: Error) -> Result<Vec<u8>, Error> {
        self.sticky = Some(error.clone());
        Err(error)
    }

    fn symbol(&mut self, byte: u8, offset: usize, out: &mut Vec<u8>) -> Result<(), Error> {
        if self.terminal {
            return Err(Error::BadPadding { offset });
        }
        let pos = self.group.len();
        if pos == 2 && self.group.get(2).map(|x| x.0) == Some(b'=') && byte != b'=' {
            return Err(Error::BadPadding { offset });
        }
        if byte == b'=' {
            if pos < 2 || pos > 3 || self.options.padding == Padding::Forbidden {
                return Err(Error::BadPadding { offset });
            }
        } else if digit(byte, self.options.alphabet).is_none() {
            return Err(Error::InvalidByte { offset, byte });
        }

        self.group.push((byte, offset));
        if self.group.len() == 4 {
            let a = digit(self.group[0].0, self.options.alphabet)
                .ok_or(Error::BadPadding { offset: self.group[0].1 })?;
            let b = digit(self.group[1].0, self.options.alphabet)
                .ok_or(Error::BadPadding { offset: self.group[1].1 })?;
            out.push((a << 2) | (b >> 4));

            if self.group[2].0 == b'=' {
                self.terminal = true;
            } else {
                let c = digit(self.group[2].0, self.options.alphabet)
                    .ok_or(Error::BadPadding { offset: self.group[2].1 })?;
                out.push((b << 4) | (c >> 2));
                if self.group[3].0 == b'=' {
                    self.terminal = true;
                } else {
                    let d = digit(self.group[3].0, self.options.alphabet)
                        .ok_or(Error::BadPadding { offset: self.group[3].1 })?;
                    out.push((c << 6) | d);
                }
            }
            self.group.clear();
        }
        Ok(())
    }

    fn process_byte(&mut self, byte: u8, offset: usize, out: &mut Vec<u8>) -> Result<(), Error> {
        match self.options.whitespace {
            Whitespace::None => self.symbol(byte, offset, out),
            Whitespace::IgnoreAscii => {
                if matches!(byte, 9 | 10 | 11 | 12 | 13 | 32) {
                    Ok(())
                } else {
                    self.symbol(byte, offset, out)
                }
            }
            Whitespace::Wrapped { width, ending } => {
                match self.frame {
                    FrameState::NeedLf(cr_offset) => {
                        if byte != b'\n' {
                            return Err(Error::BadWrap { offset });
                        }
                        self.frame = FrameState::NeedSymbol(cr_offset);
                        return Ok(());
                    }
                    FrameState::NeedSymbol(separator_offset) => {
                        if byte.is_ascii_whitespace() {
                            return Err(Error::BadWrap { offset });
                        }
                        self.frame = FrameState::Normal;
                        self.row_symbols = 0;
                        self.symbol(byte, offset, out)?;
                        self.row_symbols = 1;
                        let _ = separator_offset;
                        return Ok(());
                    }
                    FrameState::Normal => {}
                }

                if self.row_symbols == width {
                    match ending {
                        LineEnding::Lf if byte == b'\n' => {
                            self.frame = FrameState::NeedSymbol(offset);
                            return Ok(());
                        }
                        LineEnding::CrLf if byte == b'\r' => {
                            self.frame = FrameState::NeedLf(offset);
                            return Ok(());
                        }
                        _ => return Err(Error::BadWrap { offset }),
                    }
                }
                if byte.is_ascii_whitespace() {
                    return Err(Error::BadWrap { offset });
                }
                self.symbol(byte, offset, out)?;
                self.row_symbols += 1;
                Ok(())
            }
        }
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
            self.offset = self.offset.saturating_add(1);
            if let Err(error) = self.process_byte(byte, offset, &mut out) {
                return self.fail(error);
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

        if matches!(self.options.whitespace, Whitespace::Wrapped { .. }) {
            match self.frame {
                FrameState::NeedLf(cr_offset) => {
                    return self.fail(Error::BadWrap { offset: cr_offset });
                }
                FrameState::NeedSymbol(first) => {
                    return self.fail(Error::BadWrap { offset: first });
                }
                FrameState::Normal => {}
            }
        }

        if self.group.len() == 3 && self.group[2].0 == b'=' {
            return self.fail(Error::BadPadding {
                offset: self.group[2].1,
            });
        }

        if !self.group.is_empty() {
            match self.options.padding {
                Padding::Required => {
                    return self.fail(Error::Truncated {
                        offset: self.group[0].1,
                    });
                }
                Padding::Forbidden | Padding::Optional => match self.group.len() {
                    1 => {
                        return self.fail(Error::Truncated {
                            offset: self.group[0].1,
                        });
                    }
                    2 => {
                        let a = digit(self.group[0].0, self.options.alphabet).unwrap();
                        let b = digit(self.group[1].0, self.options.alphabet).unwrap();
                        self.closed = true;
                        return Ok(vec![(a << 2) | (b >> 4)]);
                    }
                    3 => {
                        let a = digit(self.group[0].0, self.options.alphabet).unwrap();
                        let b = digit(self.group[1].0, self.options.alphabet).unwrap();
                        let c = digit(self.group[2].0, self.options.alphabet).unwrap();
                        self.closed = true;
                        return Ok(vec![(a << 2) | (b >> 4), (b << 4) | (c >> 2)]);
                    }
                    _ => {}
                },
            }
        }
        self.closed = true;
        Ok(Vec::new())
    }
}

pub fn decode(input: &[u8], options: Options) -> Result<Vec<u8>, Error> {
    validate(options)?;
    let mut decoder = Decoder::new(options)?;
    let mut out = decoder.push(input)?;
    out.extend(decoder.finish()?);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(padding: Padding, whitespace: Whitespace) -> Options {
        Options {
            alphabet: Alphabet::Standard,
            padding,
            whitespace,
        }
    }

    #[test]
    fn encoding_and_alphabets() {
        let o = options(Padding::Required, Whitespace::None);
        assert_eq!(encode(b"Cat", o), Ok("Q2F0".to_owned()));
        assert_eq!(encode(b"", o), Ok(String::new()));
        let url = Options {
            alphabet: Alphabet::UrlSafe,
            padding: Padding::Forbidden,
            whitespace: Whitespace::None,
        };
        assert_eq!(encode(&[251, 255], url), Ok("-_8".to_owned()));
    }

    #[test]
    fn short_inputs_and_padding() {
        let required = options(Padding::Required, Whitespace::None);
        assert_eq!(decode(b"Zh==", required), Ok(vec![b'f']));
        assert_eq!(decode(b"Zm9=", required), Ok(b"fo".to_vec()));
        assert_eq!(decode(b"Zg=", required), Err(Error::BadPadding { offset: 2 }));
        assert_eq!(decode(b"Z", required), Err(Error::Truncated { offset: 0 }));

        let optional = options(Padding::Optional, Whitespace::None);
        assert_eq!(decode(b"Zg", optional), Ok(vec![b'f']));
        assert_eq!(decode(b"Zm8", optional), Ok(b"fo".to_vec()));
        assert_eq!(decode(b"Z", optional), Err(Error::Truncated { offset: 0 }));
    }

    #[test]
    fn streaming_and_sticky_errors() {
        let o = options(Padding::Required, Whitespace::None);
        let mut d = Decoder::new(o).unwrap();
        assert_eq!(d.push(b"T2").unwrap(), Vec::<u8>::new());
        assert_eq!(d.push(b"s=").unwrap(), b"Ok");
        assert_eq!(d.finish().unwrap(), Vec::<u8>::new());

        let mut bad = Decoder::new(o).unwrap();
        let error = bad.push(b"YWJj!").unwrap_err();
        assert_eq!(error, Error::InvalidByte { offset: 4, byte: b'!' });
        assert_eq!(bad.finish(), Err(error.clone()));
    }

    #[test]
    fn wrapped_round_trip_and_validation() {
        let wrapped = options(
            Padding::Required,
            Whitespace::Wrapped { width: 4, ending: LineEnding::CrLf },
        );
        let encoded = encode(b"abcdef", wrapped).unwrap();
        assert_eq!(encoded, "YWJj\r\nZGVm");
        assert_eq!(decode(encoded.as_bytes(), wrapped), Ok(b"abcdef".to_vec()));
        let invalid = options(
            Padding::Required,
            Whitespace::Wrapped { width: 6, ending: LineEnding::Lf },
        );
        assert_eq!(decode(b"!", invalid), Err(Error::InvalidWidth { width: 6 }));
    }

    #[test]
    fn whitespace_and_error_precedence() {
        let ignored = options(Padding::Optional, Whitespace::IgnoreAscii);
        assert_eq!(decode(b" Z g ", ignored), Ok(vec![b'f']));
        assert_eq!(decode(b"Zg== !", ignored), Err(Error::BadPadding { offset: 5 }));

        let wrapped = options(
            Padding::Required,
            Whitespace::Wrapped { width: 4, ending: LineEnding::Lf },
        );
        assert_eq!(decode(b"Zg==!", wrapped), Err(Error::BadWrap { offset: 4 }));
    }

    #[test]
    fn wrapped_chunk_boundaries() {
        let wrapped = options(
            Padding::Required,
            Whitespace::Wrapped { width: 4, ending: LineEnding::CrLf },
        );
        let mut d = Decoder::new(wrapped).unwrap();
        assert_eq!(d.push(b"YWJj\r").unwrap(), b"abc");
        assert_eq!(d.push(b"\nZGVm").unwrap(), b"def");
        assert!(d.finish().is_ok());
    }
}

fn main() {}