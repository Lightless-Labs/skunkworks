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
            return output;
        }
        output.push(payload | 0x80);
    }
}

pub fn decode_u64(input: &[u8], start: usize) -> Result<(u64, usize), Error> {
    if start > input.len() {
        return Err(Error::StartOutOfBounds { offset: start });
    }

    let mut value = 0u64;
    for index in 0..10usize {
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

    let mut values = Vec::with_capacity(count.min(input.len().saturating_sub(start)));
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
    value: u64,
    groups: usize,
    pending_len: usize,
    start: usize,
}

impl StreamDecoder {
    pub fn new() -> Self {
        Self {
            position: 0,
            value: 0,
            groups: 0,
            pending_len: 0,
            start: 0,
        }
    }

    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<u64>, Error> {
        let mut next = self.clone();
        let mut output = Vec::new();

        for &byte in chunk {
            let offset = next.position;
            let index = next.groups;

            if index == 9 {
                let payload = (byte & 0x7f) as u64;
                if payload > 1 || byte & 0x80 != 0 {
                    return Err(Error::Overflow {
                        offset,
                        start: next.start,
                    });
                }
                next.value |= payload << 63;
                next.position += 1;
                next.pending_len = 0;
                next.groups = 0;
                output.push(next.value);
                next.value = 0;
                next.start = next.position;
                continue;
            }

            let payload = (byte & 0x7f) as u64;
            next.value |= payload << (index * 7);
            next.groups += 1;
            next.pending_len += 1;
            next.position += 1;

            if byte & 0x80 == 0 {
                output.push(next.value);
                next.value = 0;
                next.groups = 0;
                next.pending_len = 0;
                next.start = next.position;
            }
        }

        *self = next;
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
        *self = Self::new();
    }
}

impl Default for StreamDecoder {
    fn default() -> Self {
        Self::new()
    }
}