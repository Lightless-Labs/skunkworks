#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    StartOutOfBounds { offset: usize },
    Truncated { offset: usize, start: usize },
    Overflow { offset: usize, start: usize },
}

pub fn encode_u64(mut value: u64) -> Vec<u8> {
    let mut output = Vec::with_capacity(10);
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
        if cursor >= input.len() {
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
        let mut next_position = self.position;
        let mut next_value = self.pending_value;
        let mut next_len = self.pending_len;
        let mut next_start = self.pending_start;
        let mut output = Vec::new();

        for &byte in chunk {
            let offset = next_position;
            if next_len == 0 {
                next_start = offset;
            }

            let payload = (byte & 0x7f) as u64;
            if next_len == 9 && (payload > 1 || byte & 0x80 != 0) {
                return Err(Error::Overflow {
                    offset,
                    start: next_start,
                });
            }

            if next_len < 9 {
                next_value |= payload << (next_len * 7);
            } else {
                next_value |= payload << 63;
            }

            next_len += 1;
            next_position += 1;

            if byte & 0x80 == 0 {
                output.push(next_value);
                next_value = 0;
                next_len = 0;
                next_start = next_position;
            }
        }

        self.position = next_position;
        self.pending_value = next_value;
        self.pending_len = next_len;
        self.pending_start = next_start;
        Ok(output)
    }

    pub fn finish(&self) -> Result<(), Error> {
        if self.pending_len == 0 {
            Ok(())
        } else {
            Err(Error::Truncated {
                offset: self.position,
                start: self.pending_start,
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