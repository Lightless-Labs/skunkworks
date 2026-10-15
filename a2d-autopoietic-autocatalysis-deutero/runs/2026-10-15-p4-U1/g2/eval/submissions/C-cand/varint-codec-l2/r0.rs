#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    StartOutOfBounds { offset: usize },
    Truncated { offset: usize, start: usize },
    Overflow { offset: usize, start: usize },
}

pub fn encode_u64(mut value: u64) -> Vec<u8> {
    let mut output = Vec::new();
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        output.push(byte);
        if value == 0 {
            return output;
        }
    }
}

pub fn decode_u64(input: &[u8], start: usize) -> Result<(u64, usize), Error> {
    if start > input.len() {
        return Err(Error::StartOutOfBounds { offset: start });
    }

    let mut value = 0u64;
    for group in 0..10 {
        let offset = start + group;
        let Some(&byte) = input.get(offset) else {
            return Err(Error::Truncated {
                offset: input.len(),
                start,
            });
        };

        let payload = (byte & 0x7f) as u64;
        if group == 9 {
            if payload > 1 || byte & 0x80 != 0 {
                return Err(Error::Overflow { offset, start });
            }
            value |= payload << 63;
            return Ok((value, offset + 1));
        }

        value |= payload << (group * 7);
        if byte & 0x80 == 0 {
            return Ok((value, offset + 1));
        }
    }

    unreachable!()
}

pub fn decode_many(
    input: &[u8],
    start: usize,
    count: usize,
) -> Result<(Vec<u64>, usize), Error> {
    if start > input.len() {
        return Err(Error::StartOutOfBounds { offset: start });
    }

    let mut values = Vec::with_capacity(count);
    let mut cursor = start;
    for _ in 0..count {
        let (value, next) = decode_u64(input, cursor)?;
        values.push(value);
        cursor = next;
    }
    Ok((values, cursor))
}

pub fn encode_i64(value: i64) -> Vec<u8> {
    let zigzag = if value >= 0 {
        (value as u64) << 1
    } else {
        ((value.unsigned_abs()) << 1).wrapping_sub(1)
    };
    encode_u64(zigzag)
}

pub fn decode_i64(input: &[u8], start: usize) -> Result<(i64, usize), Error> {
    let (value, cursor) = decode_u64(input, start)?;
    let decoded = if value & 1 == 0 {
        (value >> 1) as i64
    } else {
        -((value >> 1) as i64) - 1
    };
    Ok((decoded, cursor))
}

#[derive(Debug, Clone)]
pub struct StreamDecoder {
    position: usize,
    pending_value: u64,
    pending_len: usize,
    pending_start: usize,
}

impl StreamDecoder {
    pub fn new() -> Self {
        Self {
            position: 0,
            pending_value: 0,
            pending_len: 0,
            pending_start: 0,
        }
    }

    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<u64>, Error> {
        let mut position = self.position;
        let mut value = self.pending_value;
        let mut len = self.pending_len;
        let mut start = self.pending_start;
        let mut output = Vec::new();

        for &byte in chunk {
            if len == 0 {
                start = position;
            }

            let offset = position;
            let payload = (byte & 0x7f) as u64;
            if len == 9 {
                if payload > 1 || byte & 0x80 != 0 {
                    return Err(Error::Overflow { offset, start });
                }
                value |= payload << 63;
                output.push(value);
                value = 0;
                len = 0;
            } else {
                value |= payload << (len * 7);
                len += 1;
                if byte & 0x80 == 0 {
                    output.push(value);
                    value = 0;
                    len = 0;
                }
            }
            position += 1;
        }

        self.position = position;
        self.pending_value = value;
        self.pending_len = len;
        self.pending_start = start;
        Ok(output)
    }

    pub fn finish(&self) -> Result<(), Error> {
        if self.pending_len == 0 {
            Ok(())
        } else {
            Err(Error::Truncated {
                offset: self.position,
                start: self.position - self.pending_len,
            })
        }
    }

    pub fn position(&self) -> usize {
        self.position
    }

    pub fn pending_len(&self) -> usize {
        self.pending_len
    }

    pub fn reset(&mut self) {
        self.position = 0;
        self.pending_value = 0;
        self.pending_len = 0;
        self.pending_start = 0;
    }
}

impl Default for StreamDecoder {
    fn default() -> Self {
        Self::new()
    }
}

fn main() {}