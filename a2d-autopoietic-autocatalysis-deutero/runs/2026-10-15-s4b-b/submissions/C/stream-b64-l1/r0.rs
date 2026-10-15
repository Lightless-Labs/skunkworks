#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    InvalidByte { offset: usize, byte: u8 },
    BadPadding { offset: usize },
    Truncated { offset: usize },
}

const ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn encode(input: &[u8]) -> String {
    let mut output = String::new();
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
    match tail.len() {
        1 => {
            let a = tail[0];
            output.push(ALPHABET[(a >> 2) as usize] as char);
            output.push(ALPHABET[((a & 0x03) << 4) as usize] as char);
            output.push_str("==");
        }
        2 => {
            let a = tail[0];
            let b = tail[1];
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
    ALPHABET.iter().position(|&b| b == byte).map(|n| n as u8)
}

pub fn decode(input: &[u8]) -> Result<Vec<u8>, Error> {
    let mut output = Vec::new();
    let mut group = [0u8; 4];
    let mut offsets = [0usize; 4];
    let mut len = 0usize;
    let mut pending_second_padding: Option<usize> = None;
    let mut terminal_padding = false;

    for (offset, &byte) in input.iter().enumerate() {
        if terminal_padding {
            return Err(Error::BadPadding { offset });
        }

        if pending_second_padding.is_some() && byte != b'=' {
            return Err(Error::BadPadding { offset });
        }

        if len < 2 && byte == b'=' {
            return Err(Error::BadPadding { offset });
        }

        if byte == b'=' {
            if len == 2 {
                pending_second_padding = Some(offset);
                group[len] = 0;
                offsets[len] = offset;
                len += 1;
            } else if len == 3 {
                group[len] = 0;
                offsets[len] = offset;
                len += 1;

                if pending_second_padding.is_some() {
                    output.push((group[0] << 2) | (group[1] >> 4));
                } else {
                    output.push((group[0] << 2) | (group[1] >> 4));
                    output.push((group[1] << 4) | (group[2] >> 2));
                }
                terminal_padding = true;
                len = 0;
                pending_second_padding = None;
            } else {
                return Err(Error::BadPadding { offset });
            }
        } else {
            let value = digit(byte).ok_or(Error::InvalidByte { offset, byte })?;
            group[len] = value;
            offsets[len] = offset;
            len += 1;

            if len == 4 {
                output.push((group[0] << 2) | (group[1] >> 4));
                output.push((group[1] << 4) | (group[2] >> 2));
                output.push((group[2] << 6) | group[3]);
                len = 0;
            }
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

fn main() {}