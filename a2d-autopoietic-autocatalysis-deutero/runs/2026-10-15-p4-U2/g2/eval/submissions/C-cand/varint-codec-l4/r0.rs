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

fn encode_varint(mut value: u64, out: &mut Vec<u8>) {
    while value >= 0x80 {
        out.push((value as u8) | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

fn encode_body(message: &Message, depth: usize, max_depth: usize) -> Result<Vec<u8>, Error> {
    let mut out = Vec::new();
    let mut previous: Option<(u32, bool)> = None;

    for (index, field) in message.fields.iter().enumerate() {
        let (tag, is_bytes) = match field {
            Field::UInt { tag, .. } => (*tag, false),
            Field::Bytes { tag, .. } => (*tag, true),
            Field::Nested { tag, .. } => (*tag, false),
        };

        if !(1..=15).contains(&tag) {
            return Err(Error::InvalidTag {
                offset: index,
                tag: tag as u64,
            });
        }

        if let Some((prev_tag, prev_bytes)) = previous {
            if tag < prev_tag {
                return Err(Error::OutOfOrder {
                    offset: index,
                    previous: prev_tag,
                    tag,
                });
            }
            if tag == prev_tag && !(prev_bytes && is_bytes) {
                return Err(Error::Duplicate { offset: index, tag });
            }
        }

        match field {
            Field::UInt { value, .. } => {
                encode_varint(((tag as u64) << 3) | 0, &mut out);
                encode_varint(*value, &mut out);
            }
            Field::Bytes { data, .. } => {
                encode_varint(((tag as u64) << 3) | 2, &mut out);
                encode_varint(data.len() as u64, &mut out);
                out.extend_from_slice(data);
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
                encode_varint(((tag as u64) << 3) | 3, &mut out);
                encode_varint(child.len() as u64, &mut out);
                out.extend_from_slice(&child);
            }
        }

        previous = Some((tag, is_bytes));
    }

    Ok(out)
}

pub fn encode_message(message: &Message, max_depth: usize) -> Result<Vec<u8>, Error> {
    if max_depth > 32 {
        return Err(Error::InvalidDepthLimit { limit: max_depth });
    }
    let body = encode_body(message, 0, max_depth)?;
    let mut out = Vec::new();
    encode_varint(body.len() as u64, &mut out);
    out.extend_from_slice(&body);
    Ok(out)
}

fn read_varint(
    input: &[u8],
    cursor: &mut usize,
    end: usize,
    start: usize,
) -> Result<u64, Error> {
    let mut value = 0u64;
    for index in 0..10usize {
        if *cursor >= end {
            return Err(Error::Truncated {
                offset: end,
                start,
            });
        }
        let offset = *cursor;
        let byte = input[offset];
        *cursor += 1;
        let payload = (byte & 0x7f) as u64;

        if index == 9 && (payload > 1 || byte & 0x80 != 0) {
            return Err(Error::Overflow { offset, start });
        }

        value |= payload << (index * 7);

        if byte & 0x80 == 0 {
            if index > 0 && payload == 0 {
                return Err(Error::NonCanonical { offset: start });
            }
            return Ok(value);
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
        let key = read_varint(input, &mut cursor, end, key_start)?;
        let tag64 = key >> 3;
        let wire = (key & 7) as u8;

        if !(1..=255).contains(&tag64) {
            return Err(Error::InvalidTag {
                offset: key_start,
                tag: tag64,
            });
        }
        let tag = tag64 as u32;

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
                let value_start = cursor;
                let value = read_varint(input, &mut cursor, end, value_start)?;
                if retained {
                    fields.push(Field::UInt { tag, value });
                }
            }
            1 | 5 => {
                let width = if wire == 1 { 8usize } else { 4usize };
                if end - cursor < width {
                    return Err(Error::LengthOutOfBounds {
                        offset: key_start,
                        length: width as u64,
                    });
                }
                cursor += width;
            }
            2 => {
                let length_start = cursor;
                let length = read_varint(input, &mut cursor, end, length_start)?;
                let remaining = end - cursor;
                if length > remaining as u64 {
                    return Err(Error::LengthOutOfBounds {
                        offset: length_start,
                        length,
                    });
                }
                let data_end = cursor + length as usize;
                if retained {
                    let data = &input[cursor..data_end];
                    if let Some(Field::Bytes {
                        tag: previous_tag,
                        data: previous_data,
                    }) = fields.last_mut()
                    {
                        if *previous_tag == tag {
                            previous_data.extend_from_slice(data);
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
                cursor = data_end;
            }
            3 => {
                let length_start = cursor;
                let length = read_varint(input, &mut cursor, end, length_start)?;
                let remaining = end - cursor;
                if length > remaining as u64 {
                    return Err(Error::LengthOutOfBounds {
                        offset: length_start,
                        length,
                    });
                }
                let child_end = cursor + length as usize;
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

    let mut cursor = start;
    let length = read_varint(input, &mut cursor, input.len(), start)?;
    let header_start = start;
    let remaining = input.len() - cursor;
    if length > remaining as u64 {
        return Err(Error::LengthOutOfBounds {
            offset: header_start,
            length,
        });
    }

    let body_end = cursor + length as usize;
    let message = decode_body(input, cursor, body_end, 0, max_depth)?;
    Ok((message, body_end))
}

fn main() {}