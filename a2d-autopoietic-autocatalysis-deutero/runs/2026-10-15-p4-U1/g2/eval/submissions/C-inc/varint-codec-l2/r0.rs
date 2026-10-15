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
    for group in 0..10usize {
        let index = start + group;
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
        if byte & 0x80 == 0 {
            return Ok((value, index + 1));
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
    let encoded = if value >= 0 {
        (value as u64) << 1
    } else {
        ((value.unsigned_abs()) << 1).wrapping_sub(1)
    };
    encode_u64(encoded)
}

pub fn decode_i64(input: &[u8], start: usize) -> Result<(i64, usize), Error> {
    let (encoded, next) = decode_u64(input, start)?;
    let value = if encoded & 1 == 0 {
        (encoded >> 1) as i64
    } else {
        -((encoded >> 1) as i64) - 1
    };
    Ok((value, next))
}

pub struct StreamDecoder {
    position: usize,
    pending: Vec<u8>,
    groups: usize,
    value: u64,
    integer_start: usize,
}

impl StreamDecoder {
    pub fn new() -> Self {
        Self {
            position: 0,
            pending: Vec::new(),
            groups: 0,
            value: 0,
            integer_start: 0,
        }
    }

    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<u64>, Error> {
        let mut next = Self {
            position: self.position,
            pending: self.pending.clone(),
            groups: self.groups,
            value: self.value,
            integer_start: self.integer_start,
        };
        let mut completed = Vec::new();

        for &byte in chunk {
            let absolute = next.position;
            if next.groups == 0 {
                next.integer_start = absolute;
            }

            let payload = (byte & 0x7f) as u64;
            if next.groups == 9 {
                if payload > 1 || byte & 0x80 != 0 {
                    return Err(Error::Overflow {
                        offset: absolute,
                        start: next.integer_start,
                    });
                }
                next.value |= payload << 63;
            } else {
                next.value |= payload << (next.groups * 7);
            }

            next.pending.push(byte);
            next.position += 1;
            next.groups += 1;

            if byte & 0x80 == 0 {
                completed.push(next.value);
                next.pending.clear();
                next.groups = 0;
                next.value = 0;
            }
        }

        self.position = next.position;
        self.pending = next.pending;
        self.groups = next.groups;
        self.value = next.value;
        self.integer_start = next.integer_start;
        Ok(completed)
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
        self.groups = 0;
        self.value = 0;
        self.integer_start = 0;
    }
}

fn main() {}