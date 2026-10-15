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

fn write_varint(mut value: u64, out: &mut Vec<u8>) {
    while value >= 0x80 {
        out.push((value as u8) | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

fn read_varint(
    input: &[u8],
    mut cursor: usize,
    end: usize,
) -> Result<(u64, usize), Error> {
    let start = cursor;
    let mut value = 0u64;

    for index in 0..10 {
        if cursor >= end {
            return Err(Error::Truncated {
                offset: end,
                start,
            });
        }

        let byte = input[cursor];
        let payload = (byte & 0x7f) as u64;

        if index == 9 && (payload > 1 || byte & 0x80 != 0) {
            return Err(Error::Overflow {
                offset: cursor,
                start,
            });
        }

        value |= payload << (index * 7);
        cursor += 1;

        if byte & 0x80 == 0 {
            if index > 0 && payload == 0 {
                return Err(Error::NonCanonical { offset: start });
            }
            return Ok((value, cursor));
        }
    }

    unreachable!()
}

fn wire_of(field: &Field) -> u8 {
    match field {
        Field::UInt { .. } => 0,
        Field::Bytes { .. } => 2,
        Field::Nested { .. } => 3,
    }
}

fn tag_of(field: &Field) -> u32 {
    match field {
        Field::UInt { tag, .. }
        | Field::Bytes { tag, .. }
        | Field::Nested { tag, .. } => *tag,
    }
}

fn encode_body(
    message: &Message,
    max_depth: usize,
    depth: usize,
) -> Result<Vec<u8>, Error> {
    let mut body = Vec::new();
    let mut previous: Option<(u32, u8)> = None;

    for (index, field) in message.fields.iter().enumerate() {
        let tag = tag_of(field);
        let wire = wire_of(field);

        if !(1..=15).contains(&tag) {
            return Err(Error::InvalidTag {
                offset: index,
                tag: tag as u64,
            });
        }

        if let Some((previous_tag, previous_wire)) = previous {
            if tag < previous_tag {
                return Err(Error::OutOfOrder {
                    offset: index,
                    previous: previous_tag,
                    tag,
                });
            }
            if tag == previous_tag && !(previous_wire == 2 && wire == 2) {
                return Err(Error::Duplicate { offset: index, tag });
            }
        }

        previous = Some((tag, wire));
        write_varint(((tag as u64) << 3) | wire as u64, &mut body);

        match field {
            Field::UInt { value, .. } => write_varint(*value, &mut body),
            Field::Bytes { data, .. } => {
                write_varint(data.len() as u64, &mut body);
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
                let child = encode_body(message, max_depth, child_depth)?;
                write_varint(child.len() as u64, &mut body);
                body.extend_from_slice(&child);
            }
        }
    }

    Ok(body)
}

pub fn encode_message(message: &Message, max_depth: usize) -> Result<Vec<u8>, Error> {
    if max_depth > 32 {
        return Err(Error::InvalidDepthLimit { limit: max_depth });
    }

    let body = encode_body(message, max_depth, 0)?;
    let mut out = Vec::new();
    write_varint(body.len() as u64, &mut out);
    out.extend_from_slice(&body);
    Ok(out)
}

fn decode_body(
    input: &[u8],
    start: usize,
    end: usize,
    max_depth: usize,
    depth: usize,
) -> Result<Message, Error> {
    let mut fields = Vec::new();
    let mut cursor = start;
    let mut previous: Option<(u32, u8)> = None;

    while cursor < end {
        let key_start = cursor;
        let (key, next) = read_varint(input, cursor, end)?;
        cursor = next;

        let tag64 = key >> 3;
        let wire = (key & 7) as u8;

        if !(1..=255).contains(&tag64) {
            return Err(Error::InvalidTag {
                offset: key_start,
                tag: tag64,
            });
        }
        if !matches!(wire, 0 | 1 | 2 | 3 | 5) {
            return Err(Error::InvalidWire {
                offset: key_start,
                wire,
            });
        }

        let tag = tag64 as u32;
        if let Some((previous_tag, previous_wire)) = previous {
            if tag < previous_tag {
                return Err(Error::OutOfOrder {
                    offset: key_start,
                    previous: previous_tag,
                    tag,
                });
            }
            if tag == previous_tag && !(previous_wire == 2 && wire == 2) {
                return Err(Error::Duplicate {
                    offset: key_start,
                    tag,
                });
            }
        }
        previous = Some((tag, wire));

        match wire {
            0 => {
                let (value, next) = read_varint(input, cursor, end)?;
                cursor = next;
                if tag <= 15 {
                    fields.push(Field::UInt { tag, value });
                }
            }
            1 | 5 => {
                let length = if wire == 1 { 8usize } else { 4usize };
                let remaining = end.saturating_sub(cursor);
                if remaining < length {
                    return Err(Error::LengthOutOfBounds {
                        offset: key_start,
                        length: length as u64,
                    });
                }
                cursor += length;
            }
            2 | 3 => {
                let length_start = cursor;
                let (length, next) = read_varint(input, cursor, end)?;
                cursor = next;

                let remaining = end.saturating_sub(cursor);
                if length > remaining as u64 {
                    return Err(Error::LengthOutOfBounds {
                        offset: length_start,
                        length,
                    });
                }
                let body_length = length as usize;
                let body_start = cursor;
                let body_end = cursor + body_length;

                if wire == 2 {
                    if tag <= 15 {
                        let data = input[body_start..body_end].to_vec();
                        if let Some(Field::Bytes {
                            tag: last_tag,
                            data: last_data,
                        }) = fields.last_mut()
                        {
                            if *last_tag == tag {
                                last_data.extend_from_slice(&data);
                            } else {
                                fields.push(Field::Bytes { tag, data });
                            }
                        } else {
                            fields.push(Field::Bytes { tag, data });
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
                    let child = decode_body(input, body_start, body_end, max_depth, child_depth)?;
                    if tag <= 15 {
                        fields.push(Field::Nested {
                            tag,
                            message: child,
                        });
                    }
                }

                cursor = body_end;
            }
            _ => unreachable!(),
        }
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

    let (length, body_start) = read_varint(input, start, input.len())?;
    let available = input.len().saturating_sub(body_start);
    if length > available as u64 {
        return Err(Error::LengthOutOfBounds {
            offset: start,
            length,
        });
    }

    let body_length = length as usize;
    let body_end = body_start + body_length;
    let message = decode_body(input, body_start, body_end, max_depth, 0)?;
    Ok((message, body_end))
}