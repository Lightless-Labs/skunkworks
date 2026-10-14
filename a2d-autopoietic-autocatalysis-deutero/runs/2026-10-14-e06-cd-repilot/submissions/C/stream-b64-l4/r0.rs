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

fn alphabet_byte(alphabet: Alphabet, value: u8) -> u8 {
    match (alphabet, value) {
        (Alphabet::UrlSafe, 62) => b'-',
        (Alphabet::UrlSafe, 63) => b'_',
        (_, 62) => b'+',
        (_, 63) => b'/',
        _ if value < 26 => b'A' + value,
        _ if value < 52 => b'a' + (value - 26),
        _ => b'0' + (value - 52),
    }
}

fn digit(alphabet: Alphabet, byte: u8) -> Option<u8> {
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

fn foreign_alphabet_symbol(alphabet: Alphabet, byte: u8) -> bool {
    match alphabet {
        Alphabet::Standard => byte == b'-' || byte == b'_',
        Alphabet::UrlSafe => byte == b'+' || byte == b'/',
    }
}

fn symbols_for(bytes: &[u8], alphabet: Alphabet, padding: Padding) -> Vec<u8> {
    let mut out = Vec::new();
    for chunk in bytes.chunks(3) {
        let a = chunk[0];
        let b = *chunk.get(1).unwrap_or(&0);
        let c = *chunk.get(2).unwrap_or(&0);
        let vals = [
            a >> 2,
            ((a & 3) << 4) | (b >> 4),
            ((b & 15) << 2) | (c >> 6),
            c & 63,
        ];
        let count = match chunk.len() {
            1 => 2,
            2 => 3,
            _ => 4,
        };
        for &v in &vals[..count] {
            out.push(alphabet_byte(alphabet, v));
        }
        if padding != Padding::Forbidden {
            for _ in count..4 {
                out.push(b'=');
            }
        }
    }
    out
}

fn wrap_symbols(symbols: &[u8], whitespace: Whitespace) -> String {
    let mut out = Vec::new();
    match whitespace {
        Whitespace::Wrapped { width, ending } => {
            for (i, &byte) in symbols.iter().enumerate() {
                if i > 0 && i % width == 0 {
                    match ending {
                        LineEnding::Lf => out.push(b'\n'),
                        LineEnding::CrLf => out.extend_from_slice(b"\r\n"),
                    }
                }
                out.push(byte);
            }
        }
        _ => out.extend_from_slice(symbols),
    }
    String::from_utf8(out).unwrap_or_default()
}

/// Encodes bytes using the selected alphabet, padding, and wrapping options.
pub fn encode(input: &[u8], options: Options) -> Result<String, Error> {
    validate(options)?;
    Ok(wrap_symbols(
        &symbols_for(input, options.alphabet, options.padding),
        options.whitespace,
    ))
}

#[derive(Clone)]
struct Symbol {
    byte: u8,
    offset: usize,
    value: Option<u8>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FrameState {
    Normal,
    NeedLf { cr_offset: usize },
    NeedSymbol { separator_offset: usize },
}

fn is_ignored_ascii(byte: u8) -> bool {
    matches!(byte, 9 | 10 | 11 | 12 | 13 | 32)
}

pub struct Decoder {
    options: Options,
    offset: usize,
    symbols_seen: usize,
    group: Vec<Symbol>,
    decoded: Vec<u8>,
    padded: bool,
    frame_symbols: usize,
    frame_state: FrameState,
    sticky: Option<Error>,
    finished: bool,
}

impl Decoder {
    /// Creates an open decoder stream.
    pub fn new(options: Options) -> Result<Self, Error> {
        validate(options)?;
        Ok(Self {
            options,
            offset: 0,
            symbols_seen: 0,
            group: Vec::new(),
            decoded: Vec::new(),
            padded: false,
            frame_symbols: 0,
            frame_state: FrameState::Normal,
            sticky: None,
            finished: false,
        })
    }

    fn scan_symbol(&mut self, byte: u8, offset: usize, output: &mut Vec<u8>) -> Result<(), Error> {
        if let Some(limit) = self.options.max_symbols {
            if self.symbols_seen >= limit {
                return Err(Error::Overlong { offset, limit });
            }
        }
        self.symbols_seen += 1;

        if self.padded {
            return Err(Error::BadPadding { offset });
        }

        if self.group.len() == 3 && self.group[2].byte == b'=' && byte != b'=' {
            return Err(Error::BadPadding { offset });
        }

        if byte == b'=' {
            if self.options.padding == Padding::Forbidden || self.group.len() < 2 {
                return Err(Error::BadPadding { offset });
            }
            if self.group.len() == 3 && self.group[2].byte != b'=' {
                self.group.push(Symbol {
                    byte,
                    offset,
                    value: None,
                });
                self.complete_group(output)?;
                return Ok(());
            }
            if self.group.len() == 2 {
                self.group.push(Symbol {
                    byte,
                    offset,
                    value: None,
                });
                return Ok(());
            }
            return Err(Error::BadPadding { offset });
        }

        let value = digit(self.options.alphabet, byte);
        let value = match value {
            Some(v) => v,
            None if self.options.strict && foreign_alphabet_symbol(self.options.alphabet, byte) => {
                return Err(Error::MixedAlphabet { offset });
            }
            None => return Err(Error::InvalidByte { offset, byte }),
        };

        if self.group.len() == 3 && self.group[2].byte == b'=' {
            return Err(Error::BadPadding { offset });
        }

        self.group.push(Symbol {
            byte,
            offset,
            value: Some(value),
        });
        if self.group.len() == 4 {
            self.complete_group(output)?;
        }
        Ok(())
    }

    fn complete_group(&mut self, output: &mut Vec<u8>) -> Result<(), Error> {
        let a = self.group[0].value.unwrap();
        let b = self.group[1].value.unwrap();
        let c = self.group[2].value;
        let d = self.group[3].value;

        match (c, d) {
            (None, None) => {
                if self.options.strict && b & 15 != 0 {
                    return Err(Error::NonCanonical {
                        offset: self.group[1].offset,
                    });
                }
                output.push((a << 2) | (b >> 4));
                self.padded = true;
            }
            (Some(cv), None) => {
                if self.options.strict && cv & 3 != 0 {
                    return Err(Error::NonCanonical {
                        offset: self.group[2].offset,
                    });
                }
                output.push((a << 2) | (b >> 4));
                output.push((b << 4) | (cv >> 2));
                self.padded = true;
            }
            (Some(cv), Some(dv)) => {
                output.push((a << 2) | (b >> 4));
                output.push((b << 4) | (cv >> 2));
                output.push((cv << 6) | dv);
            }
            _ => return Err(Error::BadPadding { offset: self.group[2].offset }),
        }
        self.group.clear();
        Ok(())
    }

    fn frame_byte(&mut self, byte: u8, offset: usize, output: &mut Vec<u8>) -> Result<(), Error> {
        match self.options.whitespace {
            Whitespace::None => self.scan_symbol(byte, offset, output),
            Whitespace::IgnoreAscii => {
                if is_ignored_ascii(byte) {
                    Ok(())
                } else {
                    self.scan_symbol(byte, offset, output)
                }
            }
            Whitespace::Wrapped { width, ending } => {
                match self.frame_state {
                    FrameState::NeedLf { cr_offset } => {
                        if byte == b'\n' {
                            self.frame_state = FrameState::NeedSymbol {
                                separator_offset: cr_offset,
                            };
                            Ok(())
                        } else {
                            Err(Error::BadWrap { offset })
                        }
                    }
                    FrameState::NeedSymbol { .. } => {
                        self.frame_state = FrameState::Normal;
                        self.frame_symbols = 0;
                        if byte == b'\n' || byte == b'\r' || is_ignored_ascii(byte) {
                            return Err(Error::BadWrap { offset });
                        }
                        self.scan_symbol(byte, offset, output)
                    }
                    FrameState::Normal => {
                        if self.frame_symbols == width {
                            let separator_start = offset;
                            match ending {
                                LineEnding::Lf if byte == b'\n' => {
                                    self.frame_state = FrameState::NeedSymbol {
                                        separator_offset: separator_start,
                                    };
                                    Ok(())
                                }
                                LineEnding::CrLf if byte == b'\r' => {
                                    self.frame_state = FrameState::NeedLf {
                                        cr_offset: separator_start,
                                    };
                                    Ok(())
                                }
                                _ => Err(Error::BadWrap { offset }),
                            }
                        } else {
                            if byte == b'\n' || byte == b'\r' || is_ignored_ascii(byte) {
                                return Err(Error::BadWrap { offset });
                            }
                            self.scan_symbol(byte, offset, output)?;
                            self.frame_symbols += 1;
                            Ok(())
                        }
                    }
                }
            }
        }
    }

    /// Consumes encoded bytes and returns newly completed decoded bytes.
    pub fn push(&mut self, input: &[u8]) -> Result<Vec<u8>, Error> {
        if let Some(error) = &self.sticky {
            return Err(error.clone());
        }
        if self.finished {
            let error = Error::AlreadyFinished;
            self.sticky = Some(error.clone());
            return Err(error);
        }

        let mut output = Vec::new();
        for &byte in input {
            let offset = self.offset;
            self.offset += 1;
            if let Err(error) = self.frame_byte(byte, offset, &mut output) {
                self.sticky = Some(error.clone());
                return Err(error);
            }
        }
        self.decoded.extend_from_slice(&output);
        Ok(output)
    }

    /// Finishes decoding and emits any permitted unpadded residual group.
    pub fn finish(&mut self) -> Result<Vec<u8>, Error> {
        if let Some(error) = &self.sticky {
            return Err(error.clone());
        }
        if self.finished {
            let error = Error::AlreadyFinished;
            self.sticky = Some(error.clone());
            return Err(error);
        }

        let mut output = Vec::new();
        if let FrameState::NeedLf { cr_offset } = self.frame_state {
            return self.fail(Error::BadWrap { offset: cr_offset });
        }
        if let FrameState::NeedSymbol { separator_offset } = self.frame_state {
            return self.fail(Error::BadWrap {
                offset: separator_offset,
            });
        }

        if self.group.len() == 3 && self.group[2].byte == b'=' {
            return self.fail(Error::BadPadding {
                offset: self.group[2].offset,
            });
        }

        if !self.group.is_empty() {
            match self.options.padding {
                Padding::Required => {
                    return self.fail(Error::Truncated {
                        offset: self.group[0].offset,
                    });
                }
                Padding::Forbidden | Padding::Optional => match self.group.len() {
                    1 => {
                        return self.fail(Error::Truncated {
                            offset: self.group[0].offset,
                        });
                    }
                    2 => {
                        if self.options.strict && self.group[1].value.unwrap() & 15 != 0 {
                            return self.fail(Error::NonCanonical {
                                offset: self.group[1].offset,
                            });
                        }
                        output.push(
                            (self.group[0].value.unwrap() << 2)
                                | (self.group[1].value.unwrap() >> 4),
                        );
                    }
                    3 => {
                        if self.options.strict && self.group[2].value.unwrap() & 3 != 0 {
                            return self.fail(Error::NonCanonical {
                                offset: self.group[2].offset,
                            });
                        }
                        let a = self.group[0].value.unwrap();
                        let b = self.group[1].value.unwrap();
                        let c = self.group[2].value.unwrap();
                        output.push((a << 2) | (b >> 4));
                        output.push((b << 4) | (c >> 2));
                    }
                    _ => {}
                },
            }
        }

        self.finished = true;
        self.decoded.extend_from_slice(&output);
        Ok(output)
    }

    fn fail(&mut self, error: Error) -> Result<Vec<u8>, Error> {
        self.sticky = Some(error.clone());
        Err(error)
    }
}

/// Decodes encoded bytes according to the selected options.
pub fn decode(input: &[u8], options: Options) -> Result<Vec<u8>, Error> {
    validate(options)?;
    let mut decoder = Decoder::new(options)?;
    let mut output = decoder.push(input)?;
    output.extend(decoder.finish()?);
    Ok(output)
}

pub struct Encoder {
    options: Options,
    pending: Vec<u8>,
    emitted_symbols: usize,
    finished: bool,
}

impl Encoder {
    /// Creates an open encoder stream.
    pub fn new(options: Options) -> Result<Self, Error> {
        validate(options)?;
        Ok(Self {
            options,
            pending: Vec::new(),
            emitted_symbols: 0,
            finished: false,
        })
    }

    fn emit(&mut self, raw: &[u8]) -> String {
        let symbols = symbols_for(raw, self.options.alphabet, self.options.padding);
        let mut out = Vec::new();
        for byte in symbols {
            if let Whitespace::Wrapped { width, ending } = self.options.whitespace {
                if self.emitted_symbols > 0 && self.emitted_symbols % width == 0 {
                    match ending {
                        LineEnding::Lf => out.push(b'\n'),
                        LineEnding::CrLf => out.extend_from_slice(b"\r\n"),
                    }
                }
                self.emitted_symbols += 1;
            }
            out.push(byte);
        }
        String::from_utf8(out).unwrap_or_default()
    }

    /// Consumes input and emits all newly available complete three-byte groups.
    pub fn push(&mut self, input: &[u8]) -> Result<String, Error> {
        if self.finished {
            return Err(Error::AlreadyFinished);
        }
        self.pending.extend_from_slice(input);
        let complete = self.pending.len() / 3 * 3;
        let rest = self.pending.split_off(complete);
        let ready = std::mem::replace(&mut self.pending, rest);
        Ok(self.emit(&ready))
    }

    /// Emits the remaining tail and closes the encoder stream.
    pub fn finish(&mut self) -> Result<String, Error> {
        if self.finished {
            return Err(Error::AlreadyFinished);
        }
        self.finished = true;
        let tail = std::mem::take(&mut self.pending);
        Ok(self.emit(&tail))
    }
}

fn default_options() -> Options {
    Options {
        alphabet: Alphabet::Standard,
        padding: Padding::Required,
        whitespace: Whitespace::None,
        strict: false,
        max_symbols: None,
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_normal_and_empty() {
        assert_eq!(encode(b"Cat", default_options()).unwrap(), "Q2F0");
        assert_eq!(encode(b"", default_options()).unwrap(), "");
    }

    #[test]
    fn decode_invalid_and_padding() {
        assert_eq!(decode(b"Z", default_options()), Err(Error::Truncated { offset: 0 }));
        assert_eq!(decode(b"=AAA", default_options()), Err(Error::BadPadding { offset: 0 }));
    }

    #[test]
    fn streaming_wrapped_round_trip() {
        let options = Options {
            whitespace: Whitespace::Wrapped {
                width: 4,
                ending: LineEnding::CrLf,
            },
            ..default_options()
        };
        let mut encoder = Encoder::new(options).unwrap();
        let mut encoded = encoder.push(b"CatDog").unwrap();
        encoded.push_str(&encoder.finish().unwrap());
        assert_eq!(encoded, "Q2F0\r\nRG9n");
        assert_eq!(decode(encoded.as_bytes(), options).unwrap(), b"CatDog");
    }

    #[test]
    fn optional_unpadded_and_strict_bits() {
        let options = Options {
            padding: Padding::Optional,
            strict: true,
            ..default_options()
        };
        assert_eq!(decode(b"Zg", options).unwrap(), b"f");
        assert_eq!(
            decode(b"Zh==", options),
            Err(Error::NonCanonical { offset: 1 })
        );
    }
}