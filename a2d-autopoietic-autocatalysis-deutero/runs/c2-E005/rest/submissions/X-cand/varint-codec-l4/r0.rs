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

fn valid_depth_limit(max_depth: usize) -> Result<(), Error> {
    if max_depth > 32 {
        Err(Error::InvalidDepthLimit { limit: max_depth })
    } else {
        Ok(())
    }
}

fn encode_body(message: &Message, depth: usize, max_depth: usize) -> Result<Vec<u8>, Error> {
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
            if tag == prev_tag && (prev_wire != 2 || wire != 2) {
                return Err(Error::Duplicate { offset: index, tag });
            }
        }

        match field {
            Field::UInt { value, .. } => {
                put_varint(((tag as u64) << 3) | 0, &mut body);
                put_varint(*value, &mut body);
            }
            Field::Bytes { data, .. } => {
                put_varint(((tag as u64) << 3) | 2, &mut body);
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
                let child_body = encode_body(message, child_depth, max_depth)?;
                put_varint(((tag as u64) << 3) | 3, &mut body);
                put_varint(child_body.len() as u64, &mut body);
                body.extend_from_slice(&child_body);
            }
        }

        previous = Some((tag, wire));
    }

    Ok(body)
}

pub fn encode_message(message: &Message, max_depth: usize) -> Result<Vec<u8>, Error> {
    valid_depth_limit(max_depth)?;
    let body = encode_body(message, 0, max_depth)?;
    let mut out = Vec::new();
    put_varint(body.len() as u64, &mut out);
    out.extend_from_slice(&body);
    Ok(out)
}

fn read_varint(input: &[u8], mut cursor: usize, end: usize) -> Result<(u64, usize), Error> {
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
        cursor += 1;
        let payload = (byte & 0x7f) as u64;

        if index == 9 && (payload > 1 || (byte & 0x80) != 0) {
            return Err(Error::Overflow {
                offset: cursor - 1,
                start,
            });
        }

        value |= payload << (index * 7);

        if byte & 0x80 == 0 {
            if index > 0 && payload == 0 {
                return Err(Error::NonCanonical { offset: start });
            }
            return Ok((value, cursor));
        }
    }

    unreachable!()
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
        let (key, next) = read_varint(input, cursor, end)?;
        cursor = next;

        let tag_value = key >> 3;
        let wire = (key & 7) as u8;

        if !(1..=255).contains(&tag_value) {
            return Err(Error::InvalidTag {
                offset: key_start,
                tag: tag_value,
            });
        }
        let tag = tag_value as u32;

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
            if tag == prev_tag && (prev_wire != 2 || wire != 2) {
                return Err(Error::Duplicate {
                    offset: key_start,
                    tag,
                });
            }
        }

        let retained = tag <= 15;
        match wire {
            0 => {
                let (value, next) = read_varint(input, cursor, end)?;
                cursor = next;
                if retained {
                    fields.push(Field::UInt { tag, value });
                }
            }
            1 | 5 => {
                let size = if wire == 1 { 8usize } else { 4usize };
                if end - cursor < size {
                    return Err(Error::LengthOutOfBounds {
                        offset: key_start,
                        length: size as u64,
                    });
                }
                cursor += size;
            }
            2 => {
                let length_start = cursor;
                let (length, next) = read_varint(input, cursor, end)?;
                cursor = next;
                let remaining = end - cursor;
                if length > remaining as u64 {
                    return Err(Error::LengthOutOfBounds {
                        offset: length_start,
                        length,
                    });
                }
                let size = length as usize;
                let data = &input[cursor..cursor + size];
                if retained {
                    if let Some(Field::Bytes {
                        tag: last_tag,
                        data: last_data,
                    }) = fields.last_mut()
                    {
                        if *last_tag == tag {
                            last_data.extend_from_slice(data);
                        } else {
                            fields.push(Field::Bytes {
                                tag,
                                data: data.to_vec(),
                            });
                        }
                    } else {
                        fields.push(Field::Bytes {
                            tag,
                            data: data.to_vec(),
                        });
                    }
                }
                cursor += size;
            }
            3 => {
                let length_start = cursor;
                let (length, next) = read_varint(input, cursor, end)?;
                cursor = next;
                let remaining = end - cursor;
                if length > remaining as u64 {
                    return Err(Error::LengthOutOfBounds {
                        offset: length_start,
                        length,
                    });
                }
                let size = length as usize;
                let child_depth = depth + 1;
                if child_depth > max_depth {
                    return Err(Error::DepthExceeded {
                        offset: key_start,
                        depth: child_depth,
                    });
                }
                let child_end = cursor + size;
                let child = decode_body(input, cursor, child_end, child_depth, max_depth)?;
                cursor = child_end;
                if retained {
                    fields.push(Field::Nested {
                        tag,
                        message: child,
                    });
                }
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
    valid_depth_limit(max_depth)?;

    if start > input.len() {
        return Err(Error::StartOutOfBounds { offset: start });
    }

    let (length, body_start) = read_varint(input, start, input.len())?;
    let available = input.len() - body_start;
    if length > available as u64 {
        return Err(Error::LengthOutOfBounds {
            offset: start,
            length,
        });
    }

    let body_len = length as usize;
    let end = body_start + body_len;
    let message = decode_body(input, body_start, end, 0, max_depth)?;
    Ok((message, end))
}