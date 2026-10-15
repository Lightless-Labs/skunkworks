#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    InvalidByte { offset: usize, byte: u8 },
    BadPadding { offset: usize },
    Truncated { offset: usize },
}

const ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn digit(byte: u8) -> Option<u8> {
    ALPHABET.iter().position(|&b| b == byte).map(|i| i as u8)
}

pub fn encode(input: &[u8]) -> String {
    let mut output = String::with_capacity((input.len() + 2) / 3 * 4);
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

    match chunks.remainder() {
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

pub fn decode(input: &[u8]) -> Result<Vec<u8>, Error> {
    let mut output = Vec::new();
    let mut group: Vec<(u8, usize)> = Vec::with_capacity(4);
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

            pending_padding = None;
            terminal_padding = true;
            let a = digit(group[0].0).unwrap();
            let b = digit(group[1].0).unwrap();
            output.push((a << 2) | (b >> 4));
            group.clear();
            let _ = padding_offset;
            continue;
        }

        let position = group.len();
        if byte == b'=' {
            match position {
                0 | 1 => return Err(Error::BadPadding { offset }),
                2 => {
                    pending_padding = Some(offset);
                    group.push((byte, offset));
                }
                3 => {
                    let a = digit(group[0].0).unwrap();
                    let b = digit(group[1].0).unwrap();
                    let c = digit(group[2].0).unwrap();
                    output.push((a << 2) | (b >> 4));
                    output.push((b << 4) | (c >> 2));
                    group.clear();
                    terminal_padding = true;
                }
                _ => unreachable!(),
            }
            continue;
        }

        let value = digit(byte).ok_or(Error::InvalidByte { offset, byte })?;
        group.push((byte, offset));

        if group.len() == 4 {
            let a = digit(group[0].0).unwrap();
            let b = digit(group[1].0).unwrap();
            let c = digit(group[2].0).unwrap();
            let d = value;
            output.push((a << 2) | (b >> 4));
            output.push((b << 4) | (c >> 2));
            output.push((c << 6) | d);
            group.clear();
        }
    }

    if let Some(offset) = pending_padding {
        return Err(Error::BadPadding { offset });
    }

    if !group.is_empty() {
        return Err(Error::Truncated {
            offset: group[0].1,
        });
    }

    Ok(output)
}

fn main() {}