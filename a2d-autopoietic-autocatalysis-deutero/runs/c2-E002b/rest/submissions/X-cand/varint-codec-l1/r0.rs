#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    StartOutOfBounds { offset: usize },
    Truncated { offset: usize, start: usize },
    Overflow { offset: usize, start: usize },
}

pub fn encode_u64(mut value: u64) -> Vec<u8> {
    let mut encoded = Vec::with_capacity(10);
    loop {
        let payload = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            encoded.push(payload);
            break;
        }
        encoded.push(payload | 0x80);
    }
    encoded
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