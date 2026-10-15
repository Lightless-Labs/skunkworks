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
        let payload = byte & 0x7f;
        let continuation = byte & 0x80 != 0;

        if index == 9 {
            if payload > 1 || continuation {
                return Err(Error::Overflow { offset, start });
            }
            value |= (payload as u64) << 63;
            return Ok((value, offset + 1));
        }

        value |= (payload as u64) << (index * 7);
        if !continuation {
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
    let (value, next) = decode_u64(input, start)?;
    let decoded = if value & 1 == 0 {
        (value >> 1) as i64
    } else {
        -((value >> 1) as i64) - 1
    };
    Ok((decoded, next))
}

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
        let mut pending = self.pending.clone();
        let mut pending_start = self.pending_start;
        let mut outputs = Vec::new();
        let mut absolute = self.position;

        for &byte in chunk {
            if pending.is_empty() {
                pending_start = absolute;
            }
            pending.push(byte);

            if pending.len() == 10 {
                let payload = byte & 0x7f;
                if payload > 1 || byte & 0x80 != 0 {
                    return Err(Error::Overflow {
                        offset: absolute,
                        start: pending_start,
                    });
                }
            }

            if byte & 0x80 == 0 {
                let (value, _) = decode_u64(&pending, 0)?;
                outputs.push(value);
                pending.clear();
            }

            absolute += 1;
        }

        self.position = absolute;
        self.pending = pending;
        self.pending_start = pending_start;
        Ok(outputs)
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
        self.pending_start = 0;
    }
}

impl Default for StreamDecoder {
    fn default() -> Self {
        Self::new()
    }
}