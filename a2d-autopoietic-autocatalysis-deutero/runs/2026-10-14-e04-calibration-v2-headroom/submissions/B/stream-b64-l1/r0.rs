#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    InvalidByte { offset: usize, byte: u8 },
    BadPadding { offset: usize },
    Truncated { offset: usize },
}

pub fn encode(input: &[u8]) -> String {
    const ALPHABET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut output = String::with_capacity(input.len().div_ceil(3) * 4);

    for chunk in input.chunks(3) {
        let a = chunk[0];
        let b = chunk.get(1).copied().unwrap_or(0);
        let c = chunk.get(2).copied().unwrap_or(0);

        output.push(ALPHABET[(a >> 2) as usize] as char);
        output.push(ALPHABET[(((a & 0x03) << 4) | (b >> 4)) as usize] as char);

        if chunk.len() > 1 {
            output.push(ALPHABET[(((b & 0x0f) << 2) | (c >> 6)) as usize] as char);
        } else {
            output.push('=');
        }

        if chunk.len() > 2 {
            output.push(ALPHABET[(c & 0x3f) as usize] as char);
        } else {
            output.push('=');
        }
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
    let mut output = Vec::with_capacity(input.len() / 4 * 3);
    let mut group = [(0u8, 0usize); 4];
    let mut group_len = 0usize;
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
            group[3] = (0, offset);
            output.push((group[0].0 << 2) | (group[1].0 >> 4));
            group_len = 0;
            pending_padding = None;
            terminal_padding = true;
            let _ = padding_offset;
            continue;
        }

        if byte == b'=' {
            match group_len {
                0 | 1 => return Err(Error::BadPadding { offset }),
                2 => pending_padding = Some(offset),
                3 => {
                    let (a, _) = group[0];
                    let (b, _) = group[1];
                    let (c, _) = group[2];
                    output.push((a << 2) | (b >> 4));
                    output.push((b << 4) | (c >> 2));
                    group_len = 0;
                    terminal_padding = true;
                }
                _ => unreachable!(),
            }
            continue;
        }

        let value = digit(byte).ok_or(Error::InvalidByte { offset, byte })?;
        group[group_len] = (value, offset);
        group_len += 1;

        if group_len == 4 {
            let (a, _) = group[0];
            let (b, _) = group[1];
            let (c, _) = group[2];
            let (d, _) = group[3];
            output.push((a << 2) | (b >> 4));
            output.push((b << 4) | (c >> 2));
            output.push((c << 6) | d);
            group_len = 0;
        }
    }

    if let Some(offset) = pending_padding {
        return Err(Error::BadPadding { offset });
    }
    if group_len != 0 {
        return Err(Error::Truncated {
            offset: group[0].1,
        });
    }

    Ok(output)
}