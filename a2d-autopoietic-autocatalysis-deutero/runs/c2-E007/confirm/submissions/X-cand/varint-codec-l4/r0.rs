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

fn encode_body(message: &Message, depth: usize, max_depth: usize) -> Result<Vec<u8>, Error> {
    let mut out = Vec::new();
    let mut previous: Option<(u32, u8)> = None;

    for (index, field) in message.fields.iter().enumerate() {
        let (tag, wire) = match field {
            Field::UInt { tag, .. } => (*tag, 0),
            Field::Bytes { tag, .. } => (*tag, 2),
            Field::Nested { tag, .. } => (*tag, 3),
        };

        if !(1..=15).contains(&tag) {
            return Err(Error::InvalidTag { offset: index, tag: tag as u64 });
        }
        if let Some((prev, prev_wire)) = previous {
            if tag < prev {
                return Err(Error::OutOfOrder { offset: index, previous: prev, tag });
            }
            if tag == prev && !(prev_wire == 2 && wire == 2) {
                return Err(Error::Duplicate { offset: index, tag });
            }
        }

        let key = ((tag as u64) << 3) | wire as u64;
        put_varint(key, &mut out);
        match field {
            Field::UInt { value, .. } => put_varint(*value, &mut out),
            Field::Bytes { data, .. } => {
                put_varint(data.len() as u64, &mut out);
                out.extend_from_slice(data);
            }
            Field::Nested { message, .. } => {
                let child_depth = depth + 1;
                if child_depth > max_depth {
                    return Err(Error::DepthExceeded { offset: index, depth: child_depth });
                }
                let body = encode_body(message, child_depth, max_depth)?;
                put_varint(body.len() as u64, &mut out);
                out.extend_from_slice(&body);
            }
        }
        previous = Some((tag, wire));
    }
    Ok(out)
}

pub fn encode_message(message: &Message, max_depth: usize) -> Result<Vec<u8>, Error> {
    if max_depth > 32 {
        return Err(Error::InvalidDepthLimit { limit: max_depth });
    }
    let body = encode_body(message, 0, max_depth)?;
    let mut out = Vec::new();
    put_varint(body.len() as u64, &mut out);
    out.extend_from_slice(&body);
    Ok(out)
}

fn read_varint(input: &[u8], pos: &mut usize, end: usize, start: usize) -> Result<u64, Error> {
    let mut value = 0u64;
    for i in 0..10 {
        if *pos >= end {
            return Err(Error::Truncated { offset: end, start });
        }
        let byte = input[*pos];
        let offset = *pos;
        *pos += 1;
        let payload = byte & 0x7f;

        if i == 9 && (payload > 1 || (byte & 0x80) != 0) {
            return Err(Error::Overflow { offset, start });
        }
        value |= (payload as u64) << (7 * i);
        if byte & 0x80 == 0 {
            if i > 0 && payload == 0 {
                return Err(Error::NonCanonical { offset: start });
            }
            return Ok(value);
        }
    }
    unreachable!()
}

fn decode_body(
    input: &[u8],
    mut pos: usize,
    end: usize,
    depth: usize,
    max_depth: usize,
) -> Result<Message, Error> {
    let mut fields: Vec<Field> = Vec::new();
    let mut previous: Option<(u32, u8)> = None;

    while pos < end {
        let key_start = pos;
        let key = read_varint(input, &mut pos, end, key_start)?;
        let tag64 = key >> 3;
        let wire = (key & 7) as u8;

        if !(1..=255).contains(&tag64) {
            return Err(Error::InvalidTag { offset: key_start, tag: tag64 });
        }
        let tag = tag64 as u32;
        if !matches!(wire, 0 | 1 | 2 | 3 | 5) {
            return Err(Error::InvalidWire { offset: key_start, wire });
        }
        if let Some((prev, prev_wire)) = previous {
            if tag < prev {
                return Err(Error::OutOfOrder { offset: key_start, previous: prev, tag });
            }
            if tag == prev && !(prev_wire == 2 && wire == 2) {
                return Err(Error::Duplicate { offset: key_start, tag });
            }
        }

        let retained = tag <= 15;
        match wire {
            0 => {
                let value_start = pos;
                let value = read_varint(input, &mut pos, end, value_start)?;
                if retained {
                    fields.push(Field::UInt { tag, value });
                }
            }
            1 | 5 => {
                let length = if wire == 1 { 8 } else { 4 };
                if end - pos < length {
                    return Err(Error::LengthOutOfBounds {
                        offset: key_start,
                        length: length as u64,
                    });
                }
                pos += length;
            }
            2 | 3 => {
                let length_start = pos;
                let length = read_varint(input, &mut pos, end, length_start)?;
                let remaining = end - pos;
                if length > remaining as u64 {
                    return Err(Error::LengthOutOfBounds { offset: length_start, length });
                }
                let body_len = length as usize;
                let body_start = pos;
                let body_end = pos + body_len;
                if wire == 2 {
                    if retained {
                        let data = &input[body_start..body_end];
                        if let Some(Field::Bytes { tag: last_tag, data: last_data }) = fields.last_mut() {
                            if *last_tag == tag {
                                last_data.extend_from_slice(data);
                            } else {
                                fields.push(Field::Bytes { tag, data: data.to_vec() });
                            }
                        } else {
                            fields.push(Field::Bytes { tag, data: data.to_vec() });
                        }
                    }
                    pos = body_end;
                } else {
                    let child_depth = depth + 1;
                    if child_depth > max_depth {
                        return Err(Error::DepthExceeded { offset: key_start, depth: child_depth });
                    }
                    let child = decode_body(input, body_start, body_end, child_depth, max_depth)?;
                    pos = body_end;
                    if retained {
                        fields.push(Field::Nested { tag, message: child });
                    }
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
    if max_depth > 32 {
        return Err(Error::InvalidDepthLimit { limit: max_depth });
    }
    if start > input.len() {
        return Err(Error::StartOutOfBounds { offset: start });
    }
    let mut pos = start;
    let header_start = start;
    let length = read_varint(input, &mut pos, input.len(), header_start)?;
    let remaining = input.len() - pos;
    if length > remaining as u64 {
        return Err(Error::LengthOutOfBounds { offset: header_start, length });
    }
    let body_end = pos + length as usize;
    let message = decode_body(input, pos, body_end, 0, max_depth)?;
    Ok((message, body_end))
}