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
        let a = chunk[0];
        let b = chunk[1];
        let c = chunk[2];

        output.push(ALPHABET[(a >> 2) as usize] as char);
        output.push(ALPHABET[(((a & 0x03) << 4) | (b >> 4)) as usize] as char);
        output.push(ALPHABET[(((b & 0x0f) << 2) | (c >> 6)) as usize] as char);
        output.push(ALPHABET[(c & 0x3f) as usize] as char);
    }

    let remainder = chunks.remainder();
    match remainder {
        [a] => {
            output.push(ALPHABET[(a >> 2) as usize] as char);
            output.push(ALPHABET[((a & 0x03) << 4) as usize] as char);
            output.push_str("==");
        }
        [a, b] => {
            output.push(ALPHABET[(a >> 2) as usize] as char);
            output.push(ALPHABET[(((a & 0x03) << 4) | (b >> 4)) as usize] as char);
            output.push(ALPHABET[((b & 0x0f) << 2) as usize] as char);
            output.push('=');
        }
        _ => {}
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
    let mut group = [0u8; 4];
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
            group[3] = 0;
            offsets[3] = offset;
            let a = group[0];
            let b = group[1];
            output.push((a << 2) | (b >> 4));
            count = 0;
            pending_padding = None;
            terminal_padding = true;
            let _ = padding_offset;
            continue;
        }

        let position = count;
        if byte == b'=' {
            match position {
                0 | 1 => return Err(Error::BadPadding { offset }),
                2 => {
                    pending_padding = Some(offset);
                    group[2] = 0;
                    offsets[2] = offset;
                    count = 3;
                }
                3 => {
                    let a = group[0];
                    let b = group[1];
                    let c = group[2];
                    output.push((a << 2) | (b >> 4));
                    output.push((b << 4) | (c >> 2));
                    count = 0;
                    terminal_padding = true;
                }
                _ => unreachable!(),
            }
            continue;
        }

        let value = digit(byte).ok_or(Error::InvalidByte { offset, byte })?;
        group[position] = value;
        offsets[position] = offset;
        count += 1;

        if count == 4 {
            let a = group[0];
            let b = group[1];
            let c = group[2];
            let d = group[3];
            output.push((a << 2) | (b >> 4));
            output.push((b << 4) | (c >> 2));
            output.push((c << 6) | d);
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