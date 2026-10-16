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

fn put_varint(mut value: u64, out: &mut Vec<u8>) {
    while value >= 0x80 {
        out.push((value as u8) | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

fn get_varint(
    input: &[u8],
    mut cursor: usize,
    end: usize,
) -> Result<(u64, usize), Error> {
    let start = cursor;
    let mut value = 0u64;

    for index in 0..10 {
        if cursor >= end {
            return Err(Error::Truncated { offset: end, start });
        }
        let byte = input[cursor];
        if index == 9 && (byte & 0x80 != 0 || byte & 0x7f > 1) {
            return Err(Error::Overflow {
                offset: cursor,
                start,
            });
        }
        value |= ((byte & 0x7f) as u64) << (index * 7);
        cursor += 1;
        if byte & 0x80 == 0 {
            if index > 0 && byte & 0x7f == 0 {
                return Err(Error::NonCanonical { offset: start });
            }
            return Ok((value, cursor));
        }
    }

    unreachable!()
}

fn encode_body(
    message: &Message,
    depth: usize,
    max_depth: usize,
) -> Result<Vec<u8>, Error> {
    let mut body = Vec::new();
    let mut previous: Option<(u32, u8)> = None;

    for (index, field) in message.fields.iter().enumerate() {
        let (tag, wire) = match field {
            Field::UInt { tag, .. } => (*tag, 0),
            Field::Bytes { tag, .. } => (*tag, 2),
            Field::Nested { tag, .. } => (*tag, 3),
        };

        if !(1..=15).contains(&tag) {
            return Err(Error::InvalidTag {
                offset: index,
                tag: tag as u64,
            });
        }

        if let Some((prev_tag, prev_wire)) = previous {
            if tag < prev_tag {
                return Err(Error::OutOfOrder {
                    offset: index,
                    previous: prev_tag,
                    tag,
                });
            }
            if tag == prev_tag && !(prev_wire == 2 && wire == 2) {
                return Err(Error::Duplicate {
                    offset: index,
                    tag,
                });
            }
        }

        put_varint(((tag as u64) << 3) | wire as u64, &mut body);

        match field {
            Field::UInt { value, .. } => put_varint(*value, &mut body),
            Field::Bytes { data, .. } => {
                put_varint(data.len() as u64, &mut body);
                body.extend_from_slice(data);
            }
            Field::Nested { message, .. } => {
                let child_depth = depth + 1;
                if child_depth > max_depth {
                    return Err(Error::DepthExceeded {
                        offset: index,
                        depth: child_depth,
                    });
                }
                let child = encode_body(message, child_depth, max_depth)?;
                put_varint(child.len() as u64, &mut body);
                body.extend_from_slice(&child);
            }
        }

        previous = Some((tag, wire));
    }

    Ok(body)
}

pub fn encode_message(message: &Message, max_depth: usize) -> Result<Vec<u8>, Error> {
    if max_depth > 32 {
        return Err(Error::InvalidDepthLimit { limit: max_depth });
    }
    let body = encode_body(message, 0, max_depth)?;
    let mut output = Vec::new();
    put_varint(body.len() as u64, &mut output);
    output.extend_from_slice(&body);
    Ok(output)
}

fn decode_body(
    input: &[u8],
    mut cursor: usize,
    end: usize,
    depth: usize,
    max_depth: usize,
) -> Result<Message, Error> {
    let mut fields: Vec<Field> = Vec::new();
    let mut previous: Option<(u32, u8)> = None;

    while cursor < end {
        let key_start = cursor;
        let (key, next) = get_varint(input, cursor, end)?;
        cursor = next;

        let tag_u64 = key >> 3;
        if !(1..=255).contains(&tag_u64) {
            return Err(Error::InvalidTag {
                offset: key_start,
                tag: tag_u64,
            });
        }
        let tag = tag_u64 as u32;
        let wire = (key & 7) as u8;

        if !matches!(wire, 0 | 1 | 2 | 3 | 5) {
            return Err(Error::InvalidWire {
                offset: key_start,
                wire,
            });
        }

        if let Some((prev_tag, prev_wire)) = previous {
            if tag < prev_tag {
                return Err(Error::OutOfOrder {
                    offset: key_start,
                    previous: prev_tag,
                    tag,
                });
            }
            if tag == prev_tag && !(prev_wire == 2 && wire == 2) {
                return Err(Error::Duplicate {
                    offset: key_start,
                    tag,
                });
            }
        }

        let retained = tag <= 15;
        match wire {
            0 => {
                let (value, next) = get_varint(input, cursor, end)?;
                cursor = next;
                if retained {
                    fields.push(Field::UInt { tag, value });
                }
            }
            1 | 5 => {
                let length = if wire == 1 { 8usize } else { 4usize };
                if end - cursor < length {
                    return Err(Error::LengthOutOfBounds {
                        offset: key_start,
                        length: length as u64,
                    });
                }
                cursor += length;
            }
            2 | 3 => {
                let length_start = cursor;
                let (length, next) = get_varint(input, cursor, end)?;
                cursor = next;
                let remaining = end - cursor;
                if length > remaining as u64 {
                    return Err(Error::LengthOutOfBounds {
                        offset: length_start,
                        length,
                    });
                }
                let child_end = cursor + length as usize;

                if wire == 2 {
                    if retained {
                        let bytes = input[cursor..child_end].to_vec();
                        if let Some(Field::Bytes { tag: last_tag, data }) = fields.last_mut() {
                            if *last_tag == tag {
                                data.extend_from_slice(&bytes);
                            } else {
                                fields.push(Field::Bytes { tag, data: bytes });
                            }
                        } else {
                            fields.push(Field::Bytes { tag, data: bytes });
                        }
                    }
                } else {
                    let child_depth = depth + 1;
                    if child_depth > max_depth {
                        return Err(Error::DepthExceeded {
                            offset: key_start,
                            depth: child_depth,
                        });
                    }
                    let child = decode_body(input, cursor, child_end, child_depth, max_depth)?;
                    if retained {
                        fields.push(Field::Nested {
                            tag,
                            message: child,
                        });
                    }
                }

                cursor = child_end;
            }
            _ => unreachable!(),
        }

        previous = Some((tag, wire));
    }

    Ok(Message { fields })
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

    let (length, body_start) = get_varint(input, start, input.len())?;
    let remaining = input.len() - body_start;
    if length > remaining as u64 {
        return Err(Error::LengthOutOfBounds {
            offset: start,
            length,
        });
    }

    let end = body_start + length as usize;
    let message = decode_body(input, body_start, end, 0, max_depth)?;
    Ok((message, end))
}