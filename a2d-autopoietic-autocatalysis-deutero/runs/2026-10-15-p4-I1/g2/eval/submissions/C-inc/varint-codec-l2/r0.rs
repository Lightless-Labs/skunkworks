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

    for group in 0..10 {
        if cursor >= input.len() {
            return Err(Error::Truncated {
                offset: input.len(),
                start,
            });
        }

        let byte = input[cursor];
        let payload = (byte & 0x7f) as u64;

        if group == 9 {
            if payload > 1 || byte & 0x80 != 0 {
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

#[derive(Debug, Clone)]
pub struct StreamDecoder {
    position: usize,
    pending: Vec<u8>,
    group_count: usize,
    integer_start: usize,
}

impl StreamDecoder {
    pub fn new() -> Self {
        Self {
            position: 0,
            pending: Vec::new(),
            group_count: 0,
            integer_start: 0,
        }
    }

    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<u64>, Error> {
        let mut next = self.clone();
        let mut output = Vec::new();

        for &byte in chunk {
            let absolute_offset = next.position;
            if next.group_count == 0 {
                next.integer_start = absolute_offset;
            }

            let payload = (byte & 0x7f) as u64;
            if next.group_count == 9 && (payload > 1 || byte & 0x80 != 0) {
                return Err(Error::Overflow {
                    offset: absolute_offset,
                    start: next.integer_start,
                });
            }

            next.pending.push(byte);
            next.group_count += 1;
            next.position += 1;

            if byte & 0x80 == 0 {
                let (value, _) = decode_u64(&next.pending, 0)
                    .expect("validated stream integer must decode");
                output.push(value);
                next.pending.clear();
                next.group_count = 0;
            }
        }

        *self = next;
        Ok(output)
    }

    pub fn finish(&self) -> Result<(), Error> {
        if self.pending.is_empty() {
            Ok(())
        } else {
            Ok(()).and_then(|_| {
                Err(Error::Truncated {
                    offset: self.position,
                    start: self.position - self.pending.len(),
                })
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
        self.group_count = 0;
        self.integer_start = 0;
    }
}

impl Default for StreamDecoder {
    fn default() -> Self {
        Self::new()
    }
}

fn main() {}