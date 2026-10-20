#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    StartOutOfBounds { offset: usize },
    Truncated { offset: usize, start: usize },
    Overflow { offset: usize, start: usize },
}

pub fn encode_u64(value: u64) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(10);
    let mut remaining = value;

    while remaining >= 0x80 {
        bytes.push(((remaining & 0x7f) as u8) | 0x80);
        remaining >>= 7;
    }
    bytes.push(remaining as u8);
    bytes
}

pub fn decode_u64(input: &[u8], start: usize) -> Result<(u64, usize), Error> {
    if start > input.len() {
        return Err(Error::StartOutOfBounds { offset: start });
    }

    let mut result = 0u64;
    let mut shift = 0u32;
    let mut cursor = start;

    loop {
        if cursor == input.len() {
            return Err(Error::Truncated {
                offset: input.len(),
                start,
            });
        }

        let byte = input[cursor];
        let payload = byte & 0x7f;

        if shift == 63 {
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
