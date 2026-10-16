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

fn read_varint(input: &[u8], pos: &mut usize, end: usize) -> Result<u64, Error> {
    let start = *pos;
    let mut value = 0u64;
    for i in 0..10 {
        if *pos >= end {
            return Err(Error::Truncated {
                offset: end,
                start,
            });
        }
        let byte = input[*pos];
        let offset = *pos;
        *pos += 1;
        let payload = (byte & 0x7f) as u64;

        if i == 9 && (payload > 1 || byte & 0x80 != 0) {
            return Err(Error::Overflow { offset, start });
        }

        value |= payload << (i * 7);
        if byte & 0x80 == 0 {
            if i > 0 && payload == 0 {
                return Err(Error::NonCanonical { offset: start });
            }
            return Ok(value);
        }
    }
    unreachable!()
}

pub fn encode_message(message: &Message, max_depth: usize) -> Result<Vec<u8>, Error> {
    if max_depth > 32 {
        return Err(Error::InvalidDepthLimit { limit: max_depth });
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
                if tag == prev_tag && !(prev_wire == 2 && wire == 2) {
                    return Err(Error::Duplicate { offset: index, tag });
                }
            }

            let key = ((tag as u64) << 3) | wire as u64;
            put_varint(key, &mut body);

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

    let body = encode_body(message, 0, max_depth)?;
    let mut out = Vec::new();
    put_varint(body.len() as u64, &mut out);
    out.extend_from_slice(&body);
    Ok(out)
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
    let header_start = pos;
    let declared_len = read_varint(input, &mut pos, input.len())?;
    let remaining = input.len() - pos;
    if declared_len > remaining as u64 {
        return Err(Error::LengthOutOfBounds {
            offset: header_start,
            length: declared_len,
        });
    }
    let body_len = declared_len as usize;
    let body_end = pos + body_len;

    fn decode_body(
        input: &[u8],
        mut pos: usize,
        end: usize,
        depth: usize,
        max_depth: usize,
    ) -> Result<Message, Error> {
        let mut fields = Vec::new();
        let mut previous: Option<(u32, u8)> = None;

        while pos < end {
            let key_start = pos;
            let key = read_varint(input, &mut pos, end)?;
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
                    let value = read_varint(input, &mut pos, end)?;
                    if retained {
                        fields.push(Field::UInt { tag, value });
                    }
                }
                1 | 5 => {
                    let n = if wire == 1 { 8usize } else { 4usize };
                    if end - pos < n {
                        return Err(Error::LengthOutOfBounds {
                            offset: key_start,
                            length: n as u64,
                        });
                    }
                    pos += n;
                }
                2 | 3 => {
                    let length_start = pos;
                    let length = read_varint(input, &mut pos, end)?;
                    let remaining = end - pos;
                    if length > remaining as u64 {
                        return Err(Error::LengthOutOfBounds {
                            offset: length_start,
                            length,
                        });
                    }
                    let n = length as usize;
                    let data_start = pos;
                    let data_end = pos + n;

                    if wire == 2 {
                        if retained {
                            let data = input[data_start..data_end].to_vec();
                            if let Some(Field::Bytes {
                                tag: prior_tag,
                                data: prior_data,
                            }) = fields.last_mut()
                            {
                                if *prior_tag == tag {
                                    prior_data.extend_from_slice(&data);
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
                        let child = decode_body(input, data_start, data_end, child_depth, max_depth)?;
                        if retained {
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

            previous = Some((tag, wire));
        }

        Ok(Message { fields })
    }

    let message = decode_body(input, pos, body_end, 0, max_depth)?;
    Ok((message, body_end))
}