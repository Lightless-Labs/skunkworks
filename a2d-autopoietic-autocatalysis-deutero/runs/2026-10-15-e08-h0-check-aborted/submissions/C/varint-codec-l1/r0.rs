#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    StartOutOfBounds { offset: usize },
    Truncated { offset: usize, start: usize },
    Overflow { offset: usize, start: usize },
}

pub fn encode_u64(mut value: u64) -> Vec<u8> {
    let mut output = Vec::with_capacity(10);
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        output.push(byte);
        if value == 0 {
            return output;
        }
    }
}

pub fn decode_u64(input: &[u8], start: usize) -> Result<(u64, usize), Error> {
    if start > input.len() {
        return Err(Error::StartOutOfBounds { offset: start });
    }

    let mut value = 0u64;
    for index in 0..10 {
        let offset = match start.checked_add(index) {
            Some(offset) => offset,
            None => {
                return Err(Error::Truncated {
                    offset: input.len(),
                    start,
                });
            }
        };

        let byte = match input.get(offset) {
            Some(&byte) => byte,
            None => {
                return Err(Error::Truncated {
                    offset: input.len(),
                    start,
                });
            }
        };

        let payload = (byte & 0x7f) as u64;
        if index == 9 {
            if payload > 1 || byte & 0x80 != 0 {
                return Err(Error::Overflow { offset, start });
            }
            value |= payload << 63;
            return Ok((value, offset + 1));
        }

        value |= payload << (index * 7);
        if byte & 0x80 == 0 {
            return Ok((value, offset + 1));
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

    let mut values = Vec::new();
    let mut cursor = start;
    for _ in 0..count {
        let (value, next) = decode_u64(input, cursor)?;
        values.push(value);
        cursor = next;
    }
    Ok((values, cursor))
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips_values_across_u64_domain() {
        for value in [0, 1, 127, 128, 129, 12345, u64::MAX] {
            let encoded = encode_u64(value);
            assert!(!encoded.is_empty());
            assert_eq!(decode_u64(&encoded, 0), Ok((value, encoded.len())));
        }
        assert_eq!(encode_u64(12345), vec![0xb9, 0x60]);
        assert_eq!(encode_u64(u64::MAX).len(), 10);
    }

    #[test]
    fn decodes_from_cursor_and_stops_at_terminator() {
        assert_eq!(decode_u64(&[9, 0xac, 2, 7], 1), Ok((300, 3)));
    }

    #[test]
    fn accepts_nonminimal_encodings() {
        assert_eq!(decode_u64(&[0x80, 0], 0), Ok((0, 2)));
        assert_eq!(
            decode_u64(&[0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0], 0),
            Ok((0, 10))
        );
    }

    #[test]
    fn validates_starts_and_reports_truncation() {
        assert_eq!(
            decode_u64(&[77, 130, 129], 1),
            Err(Error::Truncated { offset: 3, start: 1 })
        );
        assert_eq!(
            decode_u64(&[], 0),
            Err(Error::Truncated { offset: 0, start: 0 })
        );
        assert_eq!(
            decode_u64(&[1], 2),
            Err(Error::StartOutOfBounds { offset: 2 })
        );
    }

    #[test]
    fn reports_tenth_byte_overflow() {
        let mut bytes = vec![0x80; 9];
        bytes.push(2);
        assert_eq!(
            decode_u64(&bytes, 0),
            Err(Error::Overflow { offset: 9, start: 0 })
        );

        bytes[9] = 0x80;
        assert_eq!(
            decode_u64(&bytes, 0),
            Err(Error::Overflow { offset: 9, start: 0 })
        );
    }

    #[test]
    fn decodes_batches_and_preserves_error_cursor() {
        assert_eq!(
            decode_many(&[1, 128, 1, 8], 0, 2),
            Ok((vec![1, 128], 3))
        );
        assert_eq!(decode_many(&[55], 0, 0), Ok((vec![], 0)));
        assert_eq!(
            decode_many(&[1, 0x80], 0, 2),
            Err(Error::Truncated { offset: 2, start: 1 })
        );
        assert_eq!(
            decode_many(&[], 1, 0),
            Err(Error::StartOutOfBounds { offset: 1 })
        );
    }
}