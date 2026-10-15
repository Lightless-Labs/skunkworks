#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    InvalidByte { offset: usize, byte: u8 },
    BadPadding { offset: usize },
    Truncated { offset: usize },
}

const ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn encode(input: &[u8]) -> String {
    let mut output = String::with_capacity(input.len().saturating_add(2) / 3 * 4);
    let mut chunks = input.chunks_exact(3);

    for chunk in &mut chunks {
        let n = ((chunk[0] as u32) << 16)
            | ((chunk[1] as u32) << 8)
            | (chunk[2] as u32);
        output.push(ALPHABET[((n >> 18) & 63) as usize] as char);
        output.push(ALPHABET[((n >> 12) & 63) as usize] as char);
        output.push(ALPHABET[((n >> 6) & 63) as usize] as char);
        output.push(ALPHABET[(n & 63) as usize] as char);
    }

    let tail = chunks.remainder();
    if tail.len() == 1 {
        let n = (tail[0] as u32) << 16;
        output.push(ALPHABET[((n >> 18) & 63) as usize] as char);
        output.push(ALPHABET[((n >> 12) & 63) as usize] as char);
        output.push_str("==");
    } else if tail.len() == 2 {
        let n = ((tail[0] as u32) << 16) | ((tail[1] as u32) << 8);
        output.push(ALPHABET[((n >> 18) & 63) as usize] as char);
        output.push(ALPHABET[((n >> 12) & 63) as usize] as char);
        output.push(ALPHABET[((n >> 6) & 63) as usize] as char);
        output.push('=');
    }

    output
}

fn digit(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

pub fn decode(input: &[u8]) -> Result<Vec<u8>, Error> {
    let mut output = Vec::new();
    let mut values = [0u8; 4];
    let mut offsets = [0usize; 4];
    let mut position = 0usize;
    let mut group_start = 0usize;
    let mut pending_padding = None;
    let mut terminal_padding = false;

    for (offset, &byte) in input.iter().enumerate() {
        if terminal_padding {
            return Err(Error::BadPadding { offset });
        }

        if pending_padding.is_some() {
            if byte != b'=' {
                return Err(Error::BadPadding { offset });
            }
            terminal_padding = true;
            pending_padding = None;
            position = 0;
            continue;
        }

        if byte == b'=' {
            match position {
                0 | 1 => return Err(Error::BadPadding { offset }),
                2 => pending_padding = Some(offset),
                3 => {
                    let n = ((values[0] as u32) << 18)
                        | ((values[1] as u32) << 12)
                        | ((values[2] as u32) << 6);
                    output.push((n >> 16) as u8);
                    output.push((n >> 8) as u8);
                    terminal_padding = true;
                    position = 0;
                }
                _ => unreachable!(),
            }
            continue;
        }

        let Some(value) = digit(byte) else {
            return Err(Error::InvalidByte { offset, byte });
        };

        if position == 0 {
            group_start = offset;
        }
        values[position] = value;
        offsets[position] = offset;
        position += 1;

        if position == 4 {
            let n = ((values[0] as u32) << 18)
                | ((values[1] as u32) << 12)
                | ((values[2] as u32) << 6)
                | (values[3] as u32);
            output.push((n >> 16) as u8);
            output.push((n >> 8) as u8);
            output.push(n as u8);
            position = 0;
        }
    }

    if let Some(offset) = pending_padding {
        return Err(Error::BadPadding { offset });
    }
    if position != 0 {
        return Err(Error::Truncated { offset: group_start });
    }

    Ok(output)
}

fn main() {}