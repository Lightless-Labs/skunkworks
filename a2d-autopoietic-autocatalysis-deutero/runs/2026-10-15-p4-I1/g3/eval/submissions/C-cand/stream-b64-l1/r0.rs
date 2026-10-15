#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    InvalidByte { offset: usize, byte: u8 },
    BadPadding { offset: usize },
    Truncated { offset: usize },
}

const ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

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

    for (offset, &byte) in input.iter().enumerate() {
        if terminal_padding {
            return Err(Error::BadPadding { offset });
        }

        if len == 2 && group[2] == b'=' && byte != b'=' {
            return Err(Error::BadPadding { offset });
        }

        if byte == b'=' {
            if len <= 1 {
                return Err(Error::BadPadding { offset });
            }
        } else if digit(byte).is_none() {
            return Err(Error::InvalidByte { offset, byte });
        }

        group[len] = byte;
        offsets[len] = offset;
        len += 1;

        if len == 4 {
            let a = digit(group[0]).expect("validated alphabet digit");
            let b = digit(group[1]).expect("validated alphabet digit");

            match (group[2], group[3]) {
                (b'=', b'=') => {
                    output.push((a << 2) | (b >> 4));
                    terminal_padding = true;
                }
                (third, b'=') => {
                    let c = digit(third).expect("validated alphabet digit");
                    output.push((a << 2) | (b >> 4));
                    output.push((b << 4) | (c >> 2));
                    terminal_padding = true;
                }
                (third, fourth) => {
                    let c = digit(third).expect("validated alphabet digit");
                    let d = digit(fourth).expect("validated alphabet digit");
                    output.push((a << 2) | (b >> 4));
                    output.push((b << 4) | (c >> 2));
                    output.push((c << 6) | d);
                }
            }
            len = 0;
        }
    }

    if len == 0 {
        return Ok(output);
    }

    if len == 3 && group[2] == b'=' {
        return Err(Error::BadPadding { offset: offsets[2] });
    }

    Err(Error::Truncated { offset: offsets[0] })
}

fn main() {}