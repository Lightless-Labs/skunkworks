#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    StartOutOfBounds { offset: usize },
    Truncated { offset: usize, start: usize },
    Overflow { offset: usize, start: usize },
}

pub fn encode_u64(mut value: u64) -> Vec<u8> {
    let mut bytes = Vec::new();
    while value >= 128 {
        bytes.push((value as u8 & 0x7f) | 0x80);
        value >>= 7;
    }
    bytes.push(value as u8);
    bytes
}

pub fn decode_u64(input: &[u8], start: usize) -> Result<(u64, usize), Error> {
    if start > input.len() {
        return Err(Error::StartOutOfBounds { offset: start });
    }

    let mut result = 0u64;
    let mut cursor = start;
    let mut shift = 0u32;

    for group in 0..10usize {
        if cursor == input.len() {
            return Err(Error::Truncated {
                offset: input.len(),
                start,
            });
        }

        let byte = input[cursor];
        let payload = byte & 0x7f;

        if group == 9 {
            if payload > 1 || byte & 0x80 != 0 {
                return Err(Error::Overflow {
                    offset: cursor,
                    start,
                });
            }
            result |= (payload as u64) << 63;
            return Ok((result, cursor + 1));
        }

        result |= (payload as u64) << shift;
        cursor += 1;
        if byte & 0x80 == 0 {
            return Ok((result, cursor));
        }
        shift += 7;
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

    let mut decoded = Vec::new();
    let mut cursor = start;
    for _ in 0..count {
        let (value, next_cursor) = decode_u64(input, cursor)?;
        decoded.push(value);
        cursor = next_cursor;
    }
    Ok((decoded, cursor))
}

pub fn encode_i64(value: i64) -> Vec<u8> {
    let sign = if value < 0 { u64::MAX } else { 0 };
    encode_u64(((value as u64) << 1) ^ sign)
}

pub fn decode_i64(input: &[u8], start: usize) -> Result<(i64, usize), Error> {
    let (encoded, cursor) = decode_u64(input, start)?;
    let decoded = ((encoded >> 1) as i64) ^ -((encoded & 1) as i64);
    Ok((decoded, cursor))
}

pub struct StreamDecoder {
    absolute_position: usize,
    partial_value: u64,
    groups: usize,
}

impl StreamDecoder {
    pub fn new() -> Self {
        Self {
            absolute_position: 0,
            partial_value: 0,
            groups: 0,
        }
    }

    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<u64>, Error> {
        let mut next_position = self.absolute_position;
        let mut next_value = self.partial_value;
        let mut next_groups = self.groups;
        let mut completed = Vec::new();

        for &byte in chunk {
            let integer_start = next_position - next_groups;
            let offset = next_position;
            let payload = byte & 0x7f;

            if next_groups == 9 {
                if payload > 1 || byte & 0x80 != 0 {
                    return Err(Error::Overflow {
                        offset,
                        start: integer_start,
                    });
                }
                next_value |= (payload as u64) << 63;
                completed.push(next_value);
                next_value = 0;
                next_groups = 0;
            } else {
                next_value |= (payload as u64) << (next_groups * 7);
                next_groups += 1;
                if byte & 0x80 == 0 {
                    completed.push(next_value);
                    next_value = 0;
                    next_groups = 0;
                }
            }

            next_position += 1;
        }

        self.absolute_position = next_position;
        self.partial_value = next_value;
        self.groups = next_groups;
        Ok(completed)
    }

    pub fn finish(&self) -> Result<(), Error> {
        match self.groups {
            0 => Ok(()),
            pending => Err(Error::Truncated {
                offset: self.absolute_position,
                start: self.absolute_position - pending,
            }),
        }
    }

    pub fn position(&self) -> usize {
        self.absolute_position
    }

    pub fn pending_len(&self) -> usize {
        self.groups
    }

    pub fn reset(&mut self) {
        self.absolute_position = 0;
        self.partial_value = 0;
        self.groups = 0;
    }
}

impl Default for StreamDecoder {
    fn default() -> Self {
        Self::new()
    }
}
