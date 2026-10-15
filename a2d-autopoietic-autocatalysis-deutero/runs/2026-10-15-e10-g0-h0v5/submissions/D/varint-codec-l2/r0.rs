#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    StartOutOfBounds { offset: usize },
    Truncated { offset: usize, start: usize },
    Overflow { offset: usize, start: usize },
}

pub fn encode_u64(mut value: u64) -> Vec<u8> {
    let mut output = Vec::new();
    loop {
        let payload = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            output.push(payload);
            break;
        }
        output.push(payload | 0x80);
    }
    output
}

pub fn decode_u64(input: &[u8], start: usize) -> Result<(u64, usize), Error> {
    if start > input.len() {
        return Err(Error::StartOutOfBounds { offset: start });
    }

    let mut value = 0u64;
    let mut cursor = start;

    for index in 0..10 {
        if cursor == input.len() {
            return Err(Error::Truncated {
                offset: input.len(),
                start,
            });
        }

        let byte = input[cursor];
        let payload = (byte & 0x7f) as u64;

        if index == 9 {
            if payload > 1 || byte & 0x80 != 0 {
                return Err(Error::Overflow {
                    offset: cursor,
                    start,
                });
            }
            value |= payload << 63;
            return Ok((value, cursor + 1));
        }

        value |= payload << (index * 7);
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
    pending: Vec<u8>,
}

impl StreamDecoder {
    pub fn new() -> Self {
        Self {
            position: 0,
            pending: Vec::new(),
        }
    }

    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<u64>, Error> {
        let mut values = Vec::new();
        let mut pending = self.pending.clone();
        let mut position = self.position;

        for &byte in chunk {
            let byte_position = position;
            position += 1;
            pending.push(byte);

            if pending.len() == 10 {
                let start = position - pending.len();
                let tenth = pending[9];
                let payload = tenth & 0x7f;
                if payload > 1 || tenth & 0x80 != 0 {
                    return Err(Error::Overflow {
                        offset: byte_position,
                        start,
                    });
                }
            }

            if byte & 0x80 == 0 {
                let start = position - pending.len();
                let (value, consumed) = decode_u64(&pending, 0).map_err(|error| match error {
                    Error::Overflow { offset, .. } => Error::Overflow {
                        offset: start + offset,
                        start,
                    },
                    Error::Truncated { offset, .. } => Error::Truncated {
                        offset: start + offset,
                        start,
                    },
                    Error::StartOutOfBounds { offset } => Error::StartOutOfBounds { offset },
                })?;
                values.push(value);
                pending.drain(..consumed);
            }
        }

        self.position = position;
        self.pending = pending;
        Ok(values)
    }

    pub fn finish(&self) -> Result<(), Error> {
        if self.pending.is_empty() {
            Ok(())
        } else {
            Err(Error::Truncated {
                offset: self.position,
                start: self.position - self.pending.len(),
            })
        }
    }

    pub fn position(&self) -> usize {
        self.position
    }

    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    pub fn reset(&mut self) {
        self.position = 0;
        self.pending.clear();
    }
}

impl Default for StreamDecoder {
    fn default() -> Self {
        Self::new()
    }
}