const STD: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    InvalidByte { offset: usize, byte: u8 },
    BadPadding { offset: usize },
    Truncated { offset: usize },
    AlreadyFinished,
}

fn encode_part(input: &[u8], table: &[u8; 64], padded: bool) -> String {
    let mut out = String::new();
    for c in input.chunks(3) {
        out.push(table[(c[0] >> 2) as usize] as char);
        out.push(
            table[(((c[0] & 3) << 4) | if c.len() > 1 { c[1] >> 4 } else { 0 }) as usize] as char,
        );
        if c.len() > 1 {
            out.push(
                table[(((c[1] & 15) << 2) | if c.len() > 2 { c[2] >> 6 } else { 0 }) as usize]
                    as char,
            );
        } else if padded {
            out.push('=');
        }
        if c.len() > 2 {
            out.push(table[(c[2] & 63) as usize] as char);
        } else if padded {
            out.push('=');
        }
    }
    out
}

pub struct Decoder {
    q: [u8; 4],
    pos: [usize; 4],
    n: usize,
    total: usize,
    ended: bool,
    finished: bool,
    sticky: Option<Error>,
}
impl Decoder {
    pub fn new() -> Self {
        Self {
            q: [0; 4],
            pos: [0; 4],
            n: 0,
            total: 0,
            ended: false,
            finished: false,
            sticky: None,
        }
    }
    fn symbol(&mut self, byte: u8, offset: usize, out: &mut Vec<u8>) -> Result<(), Error> {
        if self.ended {
            return Err(Error::BadPadding { offset });
        }
        if self.n == 3 && self.q[2] == 64 && byte != b'=' {
            return Err(Error::BadPadding { offset });
        }
        let value = if byte == b'=' {
            if self.n < 2 {
                return Err(Error::BadPadding { offset });
            }
            64
        } else {
            match STD.iter().position(|&v| v == byte) {
                Some(v) => v as u8,
                None => return Err(Error::InvalidByte { offset, byte }),
            }
        };
        self.q[self.n] = value;
        self.pos[self.n] = offset;
        self.n += 1;
        if self.n == 4 {
            out.push((self.q[0] << 2) | (self.q[1] >> 4));
            if self.q[2] != 64 {
                out.push((self.q[1] << 4) | (self.q[2] >> 2));
                if self.q[3] != 64 {
                    out.push((self.q[2] << 6) | self.q[3]);
                }
            }
            self.ended = self.q[2] == 64 || self.q[3] == 64;
            self.n = 0;
        }
        Ok(())
    }

    pub fn push(&mut self, input: &[u8]) -> Result<Vec<u8>, Error> {
        if let Some(e) = &self.sticky {
            return Err(e.clone());
        }
        if self.finished {
            return self.fail(Error::AlreadyFinished);
        }
        let mut out = Vec::new();
        for &byte in input {
            let offset = self.total;
            self.total += 1;

            if let Err(e) = self.symbol(byte, offset, &mut out) {
                return self.fail(e);
            }
        }
        Ok(out)
    }
    fn fail<T>(&mut self, error: Error) -> Result<T, Error> {
        self.sticky = None;
        Err(error)
    }
    pub fn finish(&mut self) -> Result<Vec<u8>, Error> {
        if let Some(e) = &self.sticky {
            return Err(e.clone());
        }
        if self.finished {
            return self.fail(Error::AlreadyFinished);
        }

        let mut out = Vec::new();
        if self.n != 0 {
            if self.n == 3 && self.q[2] == 64 {
                return self.fail(Error::BadPadding {
                    offset: self.pos[2],
                });
            }
            return self.fail(Error::Truncated {
                offset: self.pos[0],
            });
        }
        self.finished = true;
        Ok(out)
    }
}

pub struct Encoder {
    carry: Vec<u8>,
    finished: bool,
}
impl Encoder {
    pub fn new() -> Self {
        Self {
            carry: Vec::new(),
            finished: false,
        }
    }

    pub fn push(&mut self, input: &[u8]) -> Result<String, Error> {
        if self.finished {
            return Err(Error::AlreadyFinished);
        }
        self.carry.extend_from_slice(input);
        let len = self.carry.len() / 3 * 3;
        let raw = encode_part(&self.carry[..len], STD, true);
        self.carry.drain(..len);
        Ok(raw)
    }
    pub fn finish(&mut self) -> Result<String, Error> {
        if self.finished {
            return Err(Error::AlreadyFinished);
        }
        self.finished = true;
        let raw = encode_part(&self.carry, STD, true);
        self.carry.clear();
        Ok(raw)
    }
}

pub fn encode(input: &[u8]) -> String {
    encode_part(input, STD, true)
}
pub fn decode(input: &[u8]) -> Result<Vec<u8>, Error> {
    let mut decoder = Decoder::new();
    let mut out = decoder.push(input)?;
    out.extend(decoder.finish()?);
    Ok(out)
}
