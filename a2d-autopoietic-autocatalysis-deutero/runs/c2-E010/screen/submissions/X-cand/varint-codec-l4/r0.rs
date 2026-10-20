use std::convert::TryFrom;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub fields: Vec<Field>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Field {
    UInt { tag: u32, value: u64 },
    Bytes { tag: u32, data: Vec<u8> },
    Nested { tag: u32, message: Message },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    StartOutOfBounds { offset: usize },
    Truncated { offset: usize, start: usize },
    Overflow { offset: usize, start: usize },
    LengthOutOfBounds { offset: usize, length: u64 },
    InvalidTag { offset: usize, tag: u64 },
    InvalidWire { offset: usize, wire: u8 },
    OutOfOrder { offset: usize, previous: u32, tag: u32 },
    Duplicate { offset: usize, tag: u32 },
    NonCanonical { offset: usize },
    DepthExceeded { offset: usize, depth: usize },
    InvalidDepthLimit { limit: usize },
}

fn emit_varint(mut n: u64, dst: &mut Vec<u8>) {
    loop {
        let mut b = (n & 0x7f) as u8;
        n >>= 7;
        if n != 0 {
            b |= 0x80;
        }
        dst.push(b);
        if n == 0 {
            break;
        }
    }
}

fn encode_fields(
    fields: &[Field],
    max_depth: usize,
    depth: usize,
) -> Result<Vec<u8>, Error> {
    let mut body = Vec::new();
    let mut last: Option<(u32, u8)> = None;

    for (i, f) in fields.iter().enumerate() {
        let (tag, wire) = match f {
            Field::UInt { tag, .. } => (*tag, 0u8),
            Field::Bytes { tag, .. } => (*tag, 2u8),
            Field::Nested { tag, .. } => (*tag, 3u8),
        };
        if tag == 0 || tag > 15 {
            return Err(Error::InvalidTag {
                offset: i,
                tag: u64::from(tag),
            });
        }
        if let Some((prev, prev_wire)) = last {
            if tag < prev {
                return Err(Error::OutOfOrder {
                    offset: i,
                    previous: prev,
                    tag,
                });
            }
            if tag == prev && !(prev_wire == 2 && wire == 2) {
                return Err(Error::Duplicate { offset: i, tag });
            }
        }
        last = Some((tag, wire));
        emit_varint((u64::from(tag) << 3) | u64::from(wire), &mut body);

        match f {
            Field::UInt { value, .. } => emit_varint(*value, &mut body),
            Field::Bytes { data, .. } => {
                emit_varint(data.len() as u64, &mut body);
                body.extend_from_slice(data);
            }
            Field::Nested { message, .. } => {
                let next = depth + 1;
                if next > max_depth {
                    return Err(Error::DepthExceeded {
                        offset: i,
                        depth: next,
                    });
                }
                let nested = encode_fields(&message.fields, max_depth, next)?;
                emit_varint(nested.len() as u64, &mut body);
                body.extend_from_slice(&nested);
            }
        }
    }
    Ok(body)
}

pub fn encode_message(message: &Message, max_depth: usize) -> Result<Vec<u8>, Error> {
    if max_depth > 32 {
        return Err(Error::InvalidDepthLimit { limit: max_depth });
    }
    let body = encode_fields(&message.fields, max_depth, 0)?;
    let mut framed = Vec::new();
    emit_varint(body.len() as u64, &mut framed);
    framed.extend_from_slice(&body);
    Ok(framed)
}

struct Parser<'a> {
    bytes: &'a [u8],
}

impl<'a> Parser<'a> {
    fn varint(&self, cursor: &mut usize, end: usize) -> Result<u64, Error> {
        let begin = *cursor;
        let mut n = 0u64;
        for index in 0..10usize {
            if *cursor >= end {
                return Err(Error::Truncated {
                    offset: end,
                    start: begin,
                });
            }
            let at = *cursor;
            let byte = self.bytes[at];
            *cursor += 1;
            let part = u64::from(byte & 0x7f);
            if index == 9 && (part > 1 || byte & 0x80 != 0) {
                return Err(Error::Overflow {
                    offset: at,
                    start: begin,
                });
            }
            n |= part << (index * 7);
            if byte & 0x80 == 0 {
                if index != 0 && part == 0 {
                    return Err(Error::NonCanonical { offset: begin });
                }
                return Ok(n);
            }
        }
        unreachable!()
    }

    fn body(
        &self,
        mut cursor: usize,
        end: usize,
        max_depth: usize,
        depth: usize,
        retain: bool,
    ) -> Result<Message, Error> {
        let mut result = Message { fields: Vec::new() };
        let mut last: Option<(u32, u8)> = None;

        while cursor < end {
            let key_at = cursor;
            let key = self.varint(&mut cursor, end)?;
            let tag64 = key >> 3;
            let wire = (key & 7) as u8;
            if tag64 < 1 || tag64 > 255 {
                return Err(Error::InvalidTag {
                    offset: key_at,
                    tag: tag64,
                });
            }
            let tag = tag64 as u32;
            if wire != 0 && wire != 1 && wire != 2 && wire != 3 && wire != 5 {
                return Err(Error::InvalidWire {
                    offset: key_at,
                    wire,
                });
            }
            if let Some((prev, prev_wire)) = last {
                if tag < prev {
                    return Err(Error::OutOfOrder {
                        offset: key_at,
                        previous: prev,
                        tag,
                    });
                }
                if tag == prev && !(prev_wire == 2 && wire == 2) {
                    return Err(Error::Duplicate { offset: key_at, tag });
                }
            }
            last = Some((tag, wire));
            let keep = retain && tag <= 15;

            match wire {
                0 => {
                    let value = self.varint(&mut cursor, end)?;
                    if keep {
                        result.fields.push(Field::UInt { tag, value });
                    }
                }
                1 | 5 => {
                    let width = if wire == 1 { 8usize } else { 4usize };
                    if end - cursor < width {
                        return Err(Error::LengthOutOfBounds {
                            offset: key_at,
                            length: width as u64,
                        });
                    }
                    cursor += width;
                }
                2 => {
                    let len_at = cursor;
                    let length = self.varint(&mut cursor, end)?;
                    if length > (end - cursor) as u64 {
                        return Err(Error::LengthOutOfBounds {
                            offset: len_at,
                            length,
                        });
                    }
                    let length = usize::try_from(length).map_err(|_| {
                        Error::LengthOutOfBounds {
                            offset: len_at,
                            length,
                        }
                    })?;
                    if keep {
                        if let Some(Field::Bytes { tag: prev_tag, data }) =
                            result.fields.last_mut()
                        {
                            if *prev_tag == tag {
                                data.extend_from_slice(&self.bytes[cursor..cursor + length]);
                            } else {
                                result.fields.push(Field::Bytes {
                                    tag,
                                    data: self.bytes[cursor..cursor + length].to_vec(),
                                });
                            }
                        } else {
                            result.fields.push(Field::Bytes {
                                tag,
                                data: self.bytes[cursor..cursor + length].to_vec(),
                            });
                        }
                    }
                    cursor += length;
                }
                3 => {
                    let len_at = cursor;
                    let length = self.varint(&mut cursor, end)?;
                    if length > (end - cursor) as u64 {
                        return Err(Error::LengthOutOfBounds {
                            offset: len_at,
                            length,
                        });
                    }
                    let length_usize = usize::try_from(length).map_err(|_| {
                        Error::LengthOutOfBounds {
                            offset: len_at,
                            length,
                        }
                    })?;
                    let child_depth = depth + 1;
                    if child_depth > max_depth {
                        return Err(Error::DepthExceeded {
                            offset: key_at,
                            depth: child_depth,
                        });
                    }
                    let child_end = cursor + length_usize;
                    let child = self.body(cursor, child_end, max_depth, child_depth, keep)?;
                    cursor = child_end;
                    if keep {
                        result.fields.push(Field::Nested {
                            tag,
                            message: child,
                        });
                    }
                }
                _ => unreachable!(),
            }
        }
        Ok(result)
    }
}

pub fn decode_message(
    input: &[u8],
    start: usize,
    max_depth: usize,
) -> Result<(Message, usize), Error> {
    if max_depth > 32 {
        return Err(Error::InvalidDepthLimit { limit: max_depth });
    }
    if start > input.len() {
        return Err(Error::StartOutOfBounds { offset: start });
    }

    let parser = Parser { bytes: input };
    let mut cursor = start;
    let header = cursor;
    let frame_len = parser.varint(&mut cursor, input.len())?;
    if frame_len > (input.len() - cursor) as u64 {
        return Err(Error::LengthOutOfBounds {
            offset: header,
            length: frame_len,
        });
    }
    let frame_len = usize::try_from(frame_len).map_err(|_| Error::LengthOutOfBounds {
        offset: header,
        length: frame_len,
    })?;
    let frame_end = cursor + frame_len;
    let message = parser.body(cursor, frame_end, max_depth, 0, true)?;
    Ok((message, frame_end))
}
