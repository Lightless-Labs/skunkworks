#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    InvalidByte { offset: usize, byte: u8 },
    BadPadding { offset: usize },
    Truncated { offset: usize },
}

const ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

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

pub fn encode(input: &[u8]) -> String {
    let mut output = String::with_capacity(input.len().div_ceil(3) * 4);
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
    let mut group = [0u8; 4];
    let mut offsets = [0usize; 4];
    let mut len = 0usize;
    let mut terminal_padding = false;

    for (offset, &byte) in input.iter().enumerate() {
        if terminal_padding {
            return Err(Error::BadPadding { offset });
        }

        if len == 2 && group[1] != b'=' && group[0] != b'=' && group[2] == b'=' {
            if byte != b'=' {
                return Err(Error::BadPadding { offset });
            }
            group[3] = b'=';
            offsets[3] = offset;
            output.push((digit(group[0]).unwrap() << 2) | (digit(group[1]).unwrap() >> 4));
            len = 0;
            terminal_padding = true;
            continue;
        }

        if byte == b'=' {
            match len {
                0 | 1 => return Err(Error::BadPadding { offset }),
                2 => {
                    group[2] = byte;
                    offsets[2] = offset;
                    len = 3;
                }
                3 => {
                    if group[2] == b'=' {
                        output.push(
                            (digit(group[0]).unwrap() << 2)
                                | (digit(group[1]).unwrap() >> 4),
                        );
                        len = 0;
                        terminal_padding = true;
                    } else {
                        output.push(
                            (digit(group[0]).unwrap() << 2)
                                | (digit(group[1]).unwrap() >> 4),
                        );
                        output.push(
                            (digit(group[1]).unwrap() << 4)
                                | (digit(group[2]).unwrap() >> 2),
                        );
                        len = 0;
                        terminal_padding = true;
                    }
                }
                _ => unreachable!(),
            }
            continue;
        }

        let value = match digit(byte) {
            Some(value) => value,
            None => return Err(Error::InvalidByte { offset, byte }),
        };

        if len == 3 && group[2] == b'=' {
            return Err(Error::BadPadding { offset });
        }

        group[len] = byte;
        offsets[len] = offset;
        len += 1;

        if len == 4 {
            let a = digit(group[0]).unwrap();
            let b = digit(group[1]).unwrap();
            let c = digit(group[2]).unwrap();
            let d = value;
            output.push((a << 2) | (b >> 4));
            output.push((b << 4) | (c >> 2));
            output.push((c << 6) | d);
            len = 0;
        }
    }

    if len == 3 && group[2] == b'=' {
        return Err(Error::BadPadding { offset: offsets[2] });
    }
    if len != 0 {
        return Err(Error::Truncated {
            offset: offsets[0],
        });
    }

    Ok(output)
}

fn main() {}