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
        out.push((value as u8 & 0x7f) | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

fn encode_body(message: &Message, depth: usize, max_depth: usize) -> Result<Vec<u8>, Error> {
    let mut body = Vec::new();
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
        previous = Some((tag, is_bytes));

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
                let child = encode_body(message, child_depth, max_depth)?;
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
    let body = encode_body(message, 0, max_depth)?;
    let mut result = Vec::new();
    put_varint(body.len() as u64, &mut result);
    result.extend_from_slice(&body);
    Ok(result)
}

struct Decoder<'a> {
    input: &'a [u8],
}

impl<'a> Decoder<'a> {
    fn varint(&self, pos: &mut usize, end: usize) -> Result<u64, Error> {
        let start = *pos;
        let mut value = 0u64;
        for i in 0..10 {
            if *pos >= end {
                return Err(Error::Truncated {
                    offset: end,
                    start,
                });
            }
            let byte = self.input[*pos];
            *pos += 1;
            let payload = (byte & 0x7f) as u64;
            if i == 9 && (payload > 1 || byte & 0x80 != 0) {
                return Err(Error::Overflow {
                    offset: *pos - 1,
                    start,
                });
            }
            value |= payload << (7 * i);
            if byte & 0x80 == 0 {
                if i > 0 && payload == 0 {
                    return Err(Error::NonCanonical { offset: start });
                }
                return Ok(value);
            }
        }
        unreachable!()
    }

    fn body(&self, mut pos: usize, end: usize, depth: usize) -> Result<Message, Error> {
        let mut fields = Vec::new();
        let mut previous: Option<(u32, u8)> = None;

        while pos < end {
            let key_start = pos;
            let key = self.varint(&mut pos, end)?;
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
            previous = Some((tag, wire));

            match wire {
                0 => {
                    let value = self.varint(&mut pos, end)?;
                    if tag <= 15 {
                        fields.push(Field::UInt { tag, value });
                    }
                }
                1 | 5 => {
                    let count = if wire == 1 { 8usize } else { 4usize };
                    if end - pos < count {
                        return Err(Error::LengthOutOfBounds {
                            offset: key_start,
                            length: count as u64,
                        });
                    }
                    pos += count;
                }
                2 | 3 => {
                    let length_start = pos;
                    let length = self.varint(&mut pos, end)?;
                    let remaining = end - pos;
                    if length > remaining as u64 {
                        return Err(Error::LengthOutOfBounds {
                            offset: length_start,
                            length,
                        });
                    }
                    let length = length as usize;
                    let data_start = pos;
                    let data_end = pos + length;

                    if wire == 2 {
                        if tag <= 15 {
                            let fragment = &self.input[data_start..data_end];
                            if let Some(Field::Bytes { tag: existing, data }) =
                                fields.last_mut()
                            {
                                if *existing == tag {
                                    data.extend_from_slice(fragment);
                                } else {
                                    fields.push(Field::Bytes {
                                        tag,
                                        data: fragment.to_vec(),
                                    });
                                }
                            } else {
                                fields.push(Field::Bytes {
                                    tag,
                                    data: fragment.to_vec(),
                                });
                            }
                        }
                    } else {
                        let child_depth = depth + 1;
                        if child_depth > 32 {
                            return Err(Error::DepthExceeded {
                                offset: key_start,
                                depth: child_depth,
                            });
                        }
                        // The caller's max depth is enforced by the wrapper below.
                        let child = self.body(data_start, data_end, child_depth)?;
                        if tag <= 15 {
                            fields.push(Field::Nested {
                                tag,
                                message: child,
                            });
                        }
                    }
                    pos = data_end;
                }
                _ => unreachable!(),
            }
        }
        Ok(Message { fields })
    }

    fn body_with_limit(
        &self,
        pos: usize,
        end: usize,
        depth: usize,
        max_depth: usize,
    ) -> Result<Message, Error> {
        // This variant uses the same parser while enforcing the caller's inclusive limit.
        let mut fields = Vec::new();
        let mut cursor = pos;
        let mut previous: Option<(u32, u8)> = None;

        while cursor < end {
            let key_start = cursor;
            let key = self.varint(&mut cursor, end)?;
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
                    let value = self.varint(&mut cursor, end)?;
                    if tag <= 15 {
                        fields.push(Field::UInt { tag, value });
                    }
                }
                1 | 5 => {
                    let count = if wire == 1 { 8usize } else { 4usize };
                    if end - cursor < count {
                        return Err(Error::LengthOutOfBounds { offset: key_start, length: count as u64 });
                    }
                    cursor += count;
                }
                2 | 3 => {
                    let length_start = cursor;
                    let length = self.varint(&mut cursor, end)?;
                    if length > (end - cursor) as u64 {
                        return Err(Error::LengthOutOfBounds { offset: length_start, length });
                    }
                    let length = length as usize;
                    let data_start = cursor;
                    let data_end = cursor + length;
                    if wire == 2 {
                        if tag <= 15 {
                            let fragment = &self.input[data_start..data_end];
                            if let Some(Field::Bytes { tag: existing, data }) = fields.last_mut() {
                                if *existing == tag {
                                    data.extend_from_slice(fragment);
                                } else {
                                    fields.push(Field::Bytes { tag, data: fragment.to_vec() });
                                }
                            } else {
                                fields.push(Field::Bytes { tag, data: fragment.to_vec() });
                            }
                        }
                    } else {
                        let child_depth = depth + 1;
                        if child_depth > max_depth {
                            return Err(Error::DepthExceeded { offset: key_start, depth: child_depth });
                        }
                        let child = self.body_with_limit(data_start, data_end, child_depth, max_depth)?;
                        if tag <= 15 {
                            fields.push(Field::Nested { tag, message: child });
                        }
                    }
                    cursor = data_end;
                }
                _ => unreachable!(),
            }
        }
        Ok(Message { fields })
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
    let decoder = Decoder { input };
    let mut cursor = start;
    let length = decoder.varint(&mut cursor, input.len())?;
    if length > (input.len() - cursor) as u64 {
        return Err(Error::LengthOutOfBounds {
            offset: start,
            length,
        });
    }
    let body_end = cursor + length as usize;
    let message = decoder.body_with_limit(cursor, body_end, 0, max_depth)?;
    Ok((message, body_end))
}