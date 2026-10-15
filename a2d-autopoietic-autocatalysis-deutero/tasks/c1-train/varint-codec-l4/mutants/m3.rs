#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message { pub fields: Vec<Field> }
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
fn put(mut value: u64, out: &mut Vec<u8>) {
    loop {
        let mut byte = (value & 127) as u8;
        value >>= 7;
        if value != 0 { byte |= 128; }
        out.push(byte);
        if value == 0 { break; }
    }
}
fn read(input: &[u8], cursor: &mut usize, end: usize) -> Result<u64, Error> {
    let start = *cursor;
    let mut value = 0u64;
    for group in 0..10 {
        if *cursor == end { return Err(Error::Truncated { offset: end, start }); }
        let byte = input[*cursor];
        let payload = byte & 127;
        if group == 9 && (payload > 1 || byte & 128 != 0) {
            return Err(Error::Overflow { offset: *cursor, start });
        }
        value |= (payload as u64) << (group * 7);
        *cursor += 1;
        if byte & 128 == 0 {
            if group > 0 && payload == 0 { return Err(Error::NonCanonical { offset: start }); }
            return Ok(value);
        }
    }
    unreachable!()
}
fn bounded(length: u64, cursor: usize, end: usize, offset: usize) -> Result<usize, Error> {
    if length > (end - cursor) as u64 {
        return Err(Error::LengthOutOfBounds { offset, length });
    }
    Ok(cursor + length as usize)
}
fn check_order(tag: u32, wire: u8, previous: Option<(u32, u8)>, offset: usize) -> Result<(), Error> {
    if let Some((prev, prev_wire)) = previous {
        if tag < prev { return Err(Error::OutOfOrder { offset, previous: prev, tag }); }
        if tag == prev && !(wire == 2 && prev_wire == 2) {
            return Err(Error::Duplicate { offset, tag });
        }
    }
    Ok(())
}
fn parse_body(input: &[u8], mut cursor: usize, end: usize, depth: usize, max_depth: usize) -> Result<Message, Error> {
    let mut fields = Vec::new();
    let mut previous = None;
    while cursor < end {
        let key_start = cursor;
        let key = read(input, &mut cursor, end)?;
        let raw_tag = key >> 3;
        let wire = (key & 7) as u8;
        if raw_tag == 0 || raw_tag > 255 {
            return Err(Error::InvalidTag { offset: key_start, tag: raw_tag });
        }
        let tag = raw_tag as u32;
        if !matches!(wire, 0 | 1 | 2 | 5 | 3) {
            return Err(Error::InvalidWire { offset: key_start, wire });
        }
        check_order(tag, wire, previous, key_start)?;
        let repeat = previous.map(|p: (u32,u8)| p.0 == tag).unwrap_or(false);
        previous = Some((tag, wire));
        match wire {
            0 => {
                let value = read(input, &mut cursor, end)?;
                if tag <= 15 { fields.push(Field::UInt { tag, value }); }
            }
            1 | 5 => {
                let length = if wire == 1 { 8 } else { 4 };
                cursor = bounded(length, cursor, end, key_start)?;
            }
            2 => {
                let length_start = cursor;
                let length = read(input, &mut cursor, end)?;
                let next = bounded(length, cursor, end, length_start)?;
                if tag <= 15 {
                    let data = &input[cursor..next];
                    if repeat {
                        if let Some(Field::Bytes { data: old, .. }) = fields.last_mut() { old.extend_from_slice(data); }
                    } else { fields.push(Field::Bytes { tag, data: data.to_vec() }); }
                }
                cursor = next;
            }
            3 => {
                let length_start = cursor;
                let length = read(input, &mut cursor, end)?;
                let next = bounded(length, cursor, end, length_start)?;
                if depth + 1 >= max_depth { return Err(Error::DepthExceeded { offset: key_start, depth: depth + 1 }); }
                let message = parse_body(input, cursor, next, depth + 1, max_depth)?;
                if tag <= 15 { fields.push(Field::Nested { tag, message }); }
                cursor = next;
            }
            _ => unreachable!(),
        }
    }
    Ok(Message { fields })
}
fn build_body(message: &Message, depth: usize, max_depth: usize) -> Result<Vec<u8>, Error> {
    let mut body = Vec::new();
    let mut previous = None;
    for (index, field) in message.fields.iter().enumerate() {
        let (tag, wire) = match field {
            Field::UInt { tag, .. } => (*tag, 0),
            Field::Bytes { tag, .. } => (*tag, 2),
            Field::Nested { tag, .. } => (*tag, 3),
        };
        if tag == 0 || tag > 15 { return Err(Error::InvalidTag { offset: index, tag: tag as u64 }); }
        check_order(tag, wire, previous, index)?;
        previous = Some((tag, wire));
        put(((tag as u64) << 3) | wire as u64, &mut body);
        match field {
            Field::UInt { value, .. } => put(*value, &mut body),
            Field::Bytes { data, .. } => { put(data.len() as u64, &mut body); body.extend_from_slice(data); }
            Field::Nested { message, .. } => {
                if depth + 1 > max_depth { return Err(Error::DepthExceeded { offset: index, depth: depth + 1 }); }
                let child = build_body(message, depth + 1, max_depth)?;
                put(child.len() as u64, &mut body);
                body.extend(child);
            }
        }
    }
    Ok(body)
}
pub fn encode_message(message: &Message , max_depth: usize) -> Result<Vec<u8>, Error> {
    if max_depth > 32 { return Err(Error::InvalidDepthLimit { limit: max_depth }); }
    let body = build_body(message, 0, max_depth)?;
    let mut bytes = Vec::new();
    put(body.len() as u64, &mut bytes);
    bytes.extend(body);
    Ok(bytes)
}
pub fn decode_message(input: &[u8], start: usize , max_depth: usize) -> Result<(Message, usize), Error> {
    if max_depth > 32 { return Err(Error::InvalidDepthLimit { limit: max_depth }); }
    if start > input.len() { return Err(Error::StartOutOfBounds { offset: start }); }
    let mut cursor = start;
    let length = read(input, &mut cursor, input.len())?;
    let end = bounded(length, cursor, input.len(), start)?;
    let message = parse_body(input, cursor, end, 0, max_depth)?;
    Ok((message, end))
}
