#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    StartOutOfBounds { offset: usize },
    Truncated { offset: usize, start: usize },
    Overflow { offset: usize, start: usize },
}

pub fn encode_u64(mut value: u64) -> Vec<u8> {
    let mut encoded = Vec::with_capacity(10);
    loop {
        let payload = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            encoded.push(payload);
            break;
        }
        encoded.push(payload | 0x80);
    }
    encoded
}

pub fn decode_u64(input: &[u8], start: usize) -> Result<(u64, usize), Error> {
    if start > input.len() {
        return Err(Error::StartOutOfBounds { offset: start });
    }

    let mut value = 0u64;
    let mut cursor = start;

    for group in 0..10 {
        if cursor == input.len() {
            return Err(Error::Truncated {
                offset: input.len(),
                start,
            });
        }

        let byte = input[cursor];
        let payload = (byte & 0x7f) as u64;

        if group == 9 {
            if payload > 1 || (byte & 0x80) != 0 {
                return Err(Error::Overflow {
                    offset: cursor,
                    start,
                });
            }
            value |= payload << 63;
            return Ok((value, cursor + 1));
        }

        value |= payload << (group * 7);
        cursor += 1;

        if byte & 0x80 == 0 {
            return Ok((value, cursor));
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
        ((-(value + 1)) as u64) * 2 + 1
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
    value: u64,
    pending_len: usize,
}

impl StreamDecoder {
    pub fn new() -> Self {
        Self {
            position: 0,
            value: 0,
            pending_len: 0,
        }
    }

    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<u64>, Error> {
        let mut position = self.position;
        let mut value = self.value;
        let mut pending_len = self.pending_len;
        let mut completed = Vec::new();

        for &byte in chunk {
            let group = pending_len;
            let payload = (byte & 0x7f) as u64;
            let byte_position = position;

            if group == 9 {
                if payload > 1 || (byte & 0x80) != 0 {
                    return Err(Error::Overflow {
                        offset: byte_position,
                        start: byte_position - pending_len,
                    });
                }
                value |= payload << 63;
                completed.push(value);
                value = 0;
                pending_len = 0;
            } else {
                value |= payload << (group * 7);
                pending_len += 1;
                if byte & 0x80 == 0 {
                    completed.push(value);
                    value = 0;
                    pending_len = 0;
                }
            }

            position += 1;
        }

        self.position = position;
        self.value = value;
        self.pending_len = pending_len;
        Ok(completed)
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
        self.value = 0;
        self.pending_len = 0;
    }
}

impl Default for StreamDecoder {
    fn default() -> Self {
        Self::new()
    }
}