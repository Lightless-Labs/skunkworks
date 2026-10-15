#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    InvalidByte { offset: usize, byte: u8 },
    BadPadding { offset: usize },
    Truncated { offset: usize },
}

const ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn encode(input: &[u8]) -> String {
    let groups = input.len() / 3;
    let tail = input.len() % 3;
    let mut output = String::with_capacity(groups * 4 + if tail == 0 { 0 } else { 4 });

    for chunk in input.chunks_exact(3) {
        let a = chunk[0];
        let b = chunk[1];
        let c = chunk[2];

        output.push(ALPHABET[(a >> 2) as usize] as char);
        output.push(ALPHABET[(((a & 0x03) << 4) | (b >> 4)) as usize] as char);
        output.push(ALPHABET[(((b & 0x0f) << 2) | (c >> 6)) as usize] as char);
        output.push(ALPHABET[(c & 0x3f) as usize] as char);
    }

    let remainder = &input[groups * 3..];
    if let Some(&a) = remainder.first() {
        output.push(ALPHABET[(a >> 2) as usize] as char);
        if remainder.len() == 1 {
            output.push(ALPHABET[((a & 0x03) << 4) as usize] as char);
            output.push('=');
            output.push('=');
        } else {
            let b = remainder[1];
            output.push(ALPHABET[(((a & 0x03) << 4) | (b >> 4)) as usize] as char);
            output.push(ALPHABET[((b & 0x0f) << 2) as usize] as char);
            output.push('=');
        }
    }

    output
}

fn decode_digit(byte: u8) -> Option<u8> {
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
    let mut count = 0usize;
    let mut pending_padding: Option<usize> = None;
    let mut terminal_padding = false;

    for (offset, &byte) in input.iter().enumerate() {
        if terminal_padding {
            return Err(Error::BadPadding { offset });
        }

        if let Some(padding_offset) = pending_padding {
            if byte != b'=' {
                return Err(Error::BadPadding { offset });
            }
            terminal_padding = true;
            output.push((values[0] << 2) | (values[1] >> 4));
            pending_padding = None;
            count = 0;
            let _ = padding_offset;
            continue;
        }

        if byte == b'=' {
            match count {
                0 | 1 => return Err(Error::BadPadding { offset }),
                2 => pending_padding = Some(offset),
                3 => {
                    terminal_padding = true;
                    output.push((values[0] << 2) | (values[1] >> 4));
                    output.push((values[1] << 4) | (values[2] >> 2));
                    count = 0;
                }
                _ => unreachable!(),
            }
            continue;
        }

        let Some(value) = decode_digit(byte) else {
            return Err(Error::InvalidByte { offset, byte });
        };

        values[count] = value;
        offsets[count] = offset;
        count += 1;

        if count == 4 {
            output.push((values[0] << 2) | (values[1] >> 4));
            output.push((values[1] << 4) | (values[2] >> 2));
            output.push((values[2] << 6) | values[3]);
            count = 0;
        }
    }

    if let Some(offset) = pending_padding {
        return Err(Error::BadPadding { offset });
    }
    if count != 0 {
        return Err(Error::Truncated {
            offset: offsets[0],
        });
    }

    Ok(output)
}

fn main() {}