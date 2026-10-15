#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    StartOutOfBounds { offset: usize },
    Truncated { offset: usize, start: usize },
    Overflow { offset: usize, start: usize },
}

pub fn encode_u64(mut value: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(10);
    loop {
        let payload = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            out.push(payload);
            break;
        }
        out.push(payload | 0x80);
    }
    out
}

pub fn decode_u64(input: &[u8], start: usize) -> Result<(u64, usize), Error> {
    if start > input.len() {
        return Err(Error::StartOutOfBounds { offset: start });
    }

    let mut value = 0u64;
    for index in 0..10 {
        let offset = start + index;
        if offset >= input.len() {
            return Err(Error::Truncated {
                offset: input.len(),
                start,
            });
        }

        let byte = input[offset];
        let payload = (byte & 0x7f) as u64;

        if index == 9 {
            if payload > 1 || byte & 0x80 != 0 {
                return Err(Error::Overflow { offset, start });
            }
            value |= payload << 63;
            return Ok((value, offset + 1));
        }

        value |= payload << (index * 7);
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
    let zigzag = ((value as u64) << 1) ^ ((value >> 63) as u64);
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
        let mut next_position = self.position;
        let mut next_pending = self.pending.clone();
        let mut values = Vec::new();

        for &byte in chunk {
            let absolute_offset = next_position;
            next_position += 1;
            next_pending.push(byte);

            if next_pending.len() == 10 {
                let payload = next_pending[9] & 0x7f;
                if payload > 1 || next_pending[9] & 0x80 != 0 {
                    return Err(Error::Overflow {
                        offset: absolute_offset,
                        start: absolute_offset - 9,
                    });
                }
            }

            if byte & 0x80 == 0 {
                let integer_start = next_position - next_pending.len();
                let (value, _) = decode_u64(&next_pending, 0).map_err(|error| match error {
                    Error::Overflow { offset, .. } => Error::Overflow {
                        offset: integer_start + offset,
                        start: integer_start,
                    },
                    other => other,
                })?;
                values.push(value);
                next_pending.clear();
            }
        }

        self.position = next_position;
        self.pending = next_pending;
        Ok(values)
    }

    pub fn finish(&self) -> Result<(), Error> {
        if self.pending.is_empty() {
            Ok(())
        } else {
            Ok(())
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

fn main() {}