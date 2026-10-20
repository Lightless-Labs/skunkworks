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
    let mut index = start;

    for group in 0..10usize {
        if index >= input.len() {
            return Err(Error::Truncated {
                offset: input.len(),
                start,
            });
        }

        let byte = input[index];
        let payload = (byte & 0x7f) as u64;

        if group == 9 {
            if payload > 1 || byte & 0x80 != 0 {
                return Err(Error::Overflow {
                    offset: index,
                    start,
                });
            }
            value |= payload << 63;
            return Ok((value, index + 1));
        }

        value |= payload << (group * 7);
        index += 1;
        if byte & 0x80 == 0 {
            return Ok((value, index));
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

fn zigzag_encode(value: i64) -> u64 {
    if value >= 0 {
        (value as u64) << 1
    } else {
        value.unsigned_abs().wrapping_mul(2).wrapping_sub(1)
    }
}

fn zigzag_decode(value: u64) -> i64 {
    if value & 1 == 0 {
        (value >> 1) as i64
    } else {
        -((value >> 1) as i64) - 1
    }
}

pub fn encode_i64(value: i64) -> Vec<u8> {
    encode_u64(zigzag_encode(value))
}

pub fn decode_i64(input: &[u8], start: usize) -> Result<(i64, usize), Error> {
    let (value, cursor) = decode_u64(input, start)?;
    Ok((zigzag_decode(value), cursor))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamDecoder {
    position: usize,
    pending: Vec<u8>,
    pending_start: usize,
}

impl StreamDecoder {
    pub fn new() -> Self {
        Self {
            position: 0,
            pending: Vec::new(),
            pending_start: 0,
        }
    }

    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<u64>, Error> {
        let mut position = self.position;
        let mut pending = self.pending.clone();
        let mut pending_start = self.pending_start;
        let mut output = Vec::new();

        for &byte in chunk {
            if pending.is_empty() {
                pending_start = position;
            }

            let group = pending.len();
            if group == 9 {
                let payload = byte & 0x7f;
                if payload > 1 || byte & 0x80 != 0 {
                    return Err(Error::Overflow {
                        offset: position,
                        start: pending_start,
                    });
                }
                pending.push(byte);
                let (value, _) = decode_u64(&pending, 0)?;
                output.push(value);
                pending.clear();
            } else {
                pending.push(byte);
                if byte & 0x80 == 0 {
                    let (value, _) = decode_u64(&pending, 0)?;
                    output.push(value);
                    pending.clear();
                }
            }

            position += 1;
        }

        self.position = position;
        self.pending = pending;
        self.pending_start = if self.pending.is_empty() {
            position
        } else {
            pending_start
        };
        Ok(output)
    }

    pub fn finish(&self) -> Result<(), Error> {
        if self.pending.is_empty() {
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
        self.pending.len()
    }

    pub fn reset(&mut self) {
        self.position = 0;
        self.pending.clear();
        self.pending_start = 0;
    }
}

impl Default for StreamDecoder {
    fn default() -> Self {
        Self::new()
    }
}