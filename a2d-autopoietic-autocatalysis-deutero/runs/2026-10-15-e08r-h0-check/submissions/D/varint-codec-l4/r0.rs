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

fn read_varint(input: &[u8], mut pos: usize, end: usize) -> Result<(u64, usize), Error> {
    let start = pos;
    let mut value = 0u64;
    let mut shift = 0u32;
    let mut count = 0usize;

    loop {
        if pos >= end {
            return Err(Error::Truncated { offset: end, start });
        }
        let byte = input[pos];
        count += 1;

        if count == 10 && ((byte & 0x7f) > 1 || (byte & 0x80) != 0) {
            return Err(Error::Overflow { offset: pos, start });
        }

        value |= ((byte & 0x7f) as u64) << shift;
        pos += 1;

        if byte & 0x80 == 0 {
            if count > 1 && (byte & 0x7f) == 0 {
                return Err(Error::NonCanonical { offset: start });
            }
            return Ok((value, pos));
        }

        shift += 7;
    }
}

fn encode_body(message: &Message, max_depth: usize, depth: usize) -> Result<Vec<u8>, Error> {
    let mut body = Vec::new();
    let mut previous: Option<(u32, bool)> = None;

    for (index, field) in message.fields.iter().enumerate() {
        let (tag, is_bytes) = match field {
            Field::UInt { tag, .. } => (*tag, false),
            Field::Bytes { tag, .. } => (*tag, true),
            Field::Nested { tag, .. } => (*tag, false),
        };

        if !(1..=15).contains(&tag) {
            return Err(Error::InvalidTag { offset: index, tag: tag as u64 });
        }
        if let Some((prev_tag, prev_bytes)) = previous {
            if tag < prev_tag {
                return Err(Error::OutOfOrder { offset: index, previous: prev_tag, tag });
            }
            if tag == prev_tag && !(prev_bytes && is_bytes) {
                return Err(Error::Duplicate { offset: index, tag });
            }
        }
        previous = Some((tag, is_bytes));

        match field {
            Field::UInt { value, .. } => {
                put_varint(((tag as u64) << 3), &mut body);
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
                    return Err(Error::DepthExceeded { offset: index, depth: child_depth });
                }
                let child = encode_body(message, max_depth, child_depth)?;
                put_varint(((tag as u64) << 3) | 3, &mut body);
                put_varint(child.len() as u64, &mut body);
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
    put_varint(body.len() as u64, &mut out);
    out.extend_from_slice(&body);
    Ok(out)
}

fn decode_body(
    input: &[u8],
    mut pos: usize,
    end: usize,
    max_depth: usize,
    depth: usize,
) -> Result<Message, Error> {
    let mut fields: Vec<Field> = Vec::new();
    let mut previous: Option<(u32, u8)> = None;

    while pos < end {
        let key_start = pos;
        let (key, next) = read_varint(input, pos, end)?;
        pos = next;

        let tag64 = key >> 3;
        let wire = (key & 7) as u8;
        if !(1..=255).contains(&tag64) {
            return Err(Error::InvalidTag { offset: key_start, tag: tag64 });
        }
        let tag = tag64 as u32;

        if !matches!(wire, 0 | 1 | 2 | 3 | 5) {
            return Err(Error::InvalidWire { offset: key_start, wire });
        }

        if let Some((prev_tag, prev_wire)) = previous {
            if tag < prev_tag {
                return Err(Error::OutOfOrder { offset: key_start, previous: prev_tag, tag });
            }
            if tag == prev_tag && !(prev_wire == 2 && wire == 2) {
                return Err(Error::Duplicate { offset: key_start, tag });
            }
        }
        previous = Some((tag, wire));

        match wire {
            0 => {
                let payload_start = pos;
                let (value, next) = read_varint(input, pos, end)?;
                pos = next;
                if tag <= 15 {
                    fields.push(Field::UInt { tag, value });
                }
                let _ = payload_start;
            }
            1 | 5 => {
                let length = if wire == 1 { 8usize } else { 4usize };
                let remaining = end - pos;
                if remaining < length {
                    return Err(Error::LengthOutOfBounds {
                        offset: key_start,
                        length: length as u64,
                    });
                }
                pos += length;
            }
            2 => {
                let length_start = pos;
                let (length, next) = read_varint(input, pos, end)?;
                pos = next;
                let remaining = (end - pos) as u64;
                if length > remaining {
                    return Err(Error::LengthOutOfBounds { offset: length_start, length });
                }
                let data_end = pos + length as usize;
                if tag <= 15 {
                    if let Some(Field::Bytes { tag: last_tag, data }) = fields.last_mut() {
                        if *last_tag == tag {
                            data.extend_from_slice(&input[pos..data_end]);
                        } else {
                            fields.push(Field::Bytes {
                                tag,
                                data: input[pos..data_end].to_vec(),
                            });
                        }
                    } else {
                        fields.push(Field::Bytes {
                            tag,
                            data: input[pos..data_end].to_vec(),
                        });
                    }
                }
                pos = data_end;
            }
            3 => {
                let length_start = pos;
                let (length, next) = read_varint(input, pos, end)?;
                pos = next;
                let remaining = (end - pos) as u64;
                if length > remaining {
                    return Err(Error::LengthOutOfBounds { offset: length_start, length });
                }
                let child_depth = depth + 1;
                if child_depth > max_depth {
                    return Err(Error::DepthExceeded { offset: key_start, depth: child_depth });
                }
                let child_end = pos + length as usize;
                let child = decode_body(input, pos, child_end, max_depth, child_depth)?;
                if tag <= 15 {
                    fields.push(Field::Nested { tag, message: child });
                }
                pos = child_end;
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
    let remaining = (input.len() - body_start) as u64;
    if length > remaining {
        return Err(Error::LengthOutOfBounds { offset: start, length });
    }
    let body_end = body_start + length as usize;
    let message = decode_body(input, body_start, body_end, max_depth, 0)?;
    Ok((message, body_end))
}