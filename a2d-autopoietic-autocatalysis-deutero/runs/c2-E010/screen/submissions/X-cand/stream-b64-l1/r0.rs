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

    let tail = chunks.remainder();
    if tail.len() == 1 {
        let a = tail[0];
        output.push(ALPHABET[(a >> 2) as usize] as char);
        output.push(ALPHABET[((a & 0x03) << 4) as usize] as char);
        output.push('=');
        output.push('=');
    } else if tail.len() == 2 {
        let a = tail[0];
        let b = tail[1];
        output.push(ALPHABET[(a >> 2) as usize] as char);
        output.push(ALPHABET[(((a & 0x03) << 4) | (b >> 4)) as usize] as char);
        output.push(ALPHABET[((b & 0x0f) << 2) as usize] as char);
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
    let mut group = [0u8; 4];
    let mut offsets = [0usize; 4];
    let mut len = 0usize;
    let mut terminal_padding = false;
    let mut pending_second_padding: Option<usize> = None;

    for (offset, &byte) in input.iter().enumerate() {
        if terminal_padding {
            return Err(Error::BadPadding { offset });
        }

        if let Some(padding_offset) = pending_second_padding {
            if byte != b'=' {
                return Err(Error::BadPadding { offset });
            }
            // The pending '=' is at position two; this one completes xx==.
            let a = group[0];
            let b = group[1];
            output.push((a << 2) | (b >> 4));
            let _ = padding_offset;
            pending_second_padding = None;
            len = 0;
            terminal_padding = true;
            continue;
        }

        if byte == b'=' {
            match len {
                0 | 1 => return Err(Error::BadPadding { offset }),
                2 => {
                    pending_second_padding = Some(offset);
                    group[2] = 0;
                    offsets[2] = offset;
                    len = 3;
                }
                3 => {
                    let a = group[0];
                    let b = group[1];
                    let c = group[2];
                    output.push((a << 2) | (b >> 4));
                    output.push((b << 4) | (c >> 2));
                    len = 0;
                    terminal_padding = true;
                }
                _ => unreachable!(),
            }
            continue;
        }

        let value = digit(byte).ok_or(Error::InvalidByte { offset, byte })?;
        group[len] = value;
        offsets[len] = offset;
        len += 1;

        if len == 4 {
            let a = group[0];
            let b = group[1];
            let c = group[2];
            let d = group[3];
            output.push((a << 2) | (b >> 4));
            output.push((b << 4) | (c >> 2));
            output.push((c << 6) | d);
            len = 0;
        }
    }

    if let Some(offset) = pending_second_padding {
        return Err(Error::BadPadding { offset });
    }
    if len != 0 {
        return Err(Error::Truncated {
            offset: offsets[0],
        });
    }

    Ok(output)
}
