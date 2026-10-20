#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub fields: Vec<Field>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Field {
    UInt { tag: u32, value: u64 },
    Bytes { tag: u32, data: Vec<u8> },
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
}

fn write_varint(mut value: u64, output: &mut Vec<u8>) {
    while value >= 0x80 {
        output.push((value as u8) | 0x80);
        value >>= 7;
    }
    output.push(value as u8);
}

fn read_varint(input: &[u8], cursor: &mut usize, end: usize) -> Result<u64, Error> {
    let start = *cursor;
    let mut value = 0u64;

    for index in 0..10 {
        if *cursor >= end {
            return Err(Error::Truncated {
                offset: end,
                start,
            });
        }

        let offset = *cursor;
        let byte = input[offset];
        *cursor += 1;
        let payload = byte & 0x7f;

        if index == 9 && (payload > 1 || byte & 0x80 != 0) {
            return Err(Error::Overflow { offset, start });
        }

        value |= (payload as u64) << (index * 7);
        if byte & 0x80 == 0 {
            return Ok(value);
        }
    }

    unreachable!()
}

pub fn encode_message(message: &Message) -> Result<Vec<u8>, Error> {
    let mut body = Vec::new();
    let mut previous: Option<(u32, bool)> = None;

    for (index, field) in message.fields.iter().enumerate() {
        let (tag, is_bytes) = match field {
            Field::UInt { tag, .. } => (*tag, false),
            Field::Bytes { tag, .. } => (*tag, true),
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
                return Err(Error::Duplicate {
                    offset: index,
                    tag,
                });
            }
        }

        match field {
            Field::UInt { value, .. } => {
                write_varint((tag as u64) << 3, &mut body);
                write_varint(*value, &mut body);
            }
            Field::Bytes { data, .. } => {
                write_varint(((tag as u64) << 3) | 2, &mut body);
                write_varint(data.len() as u64, &mut body);
                body.extend_from_slice(data);
            }
        }

        previous = Some((tag, is_bytes));
    }

    let mut output = Vec::new();
    write_varint(body.len() as u64, &mut output);
    output.extend_from_slice(&body);
    Ok(output)
}

pub fn decode_message(input: &[u8], start: usize) -> Result<(Message, usize), Error> {
    if start > input.len() {
        return Err(Error::StartOutOfBounds { offset: start });
    }

    let mut cursor = start;
    let header_start = start;
    let declared_length = read_varint(input, &mut cursor, input.len())?;

    let remaining = input.len() - cursor;
    if declared_length > remaining as u64 {
        return Err(Error::LengthOutOfBounds {
            offset: header_start,
            length: declared_length,
        });
    }

    let body_len = usize::try_from(declared_length).map_err(|_| Error::LengthOutOfBounds {
        offset: header_start,
        length: declared_length,
    })?;
    let frame_end = cursor + body_len;
    let mut message = Message { fields: Vec::new() };
    let mut previous: Option<(u32, u8)> = None;

    while cursor < frame_end {
        let key_start = cursor;
        let key = read_varint(input, &mut cursor, frame_end)?;
        let tag = key >> 3;
        let wire = (key & 7) as u8;

        if !(1..=255).contains(&tag) {
            return Err(Error::InvalidTag {
                offset: key_start,
                tag,
            });
        }
        if !matches!(wire, 0 | 1 | 2 | 5) {
            return Err(Error::InvalidWire {
                offset: key_start,
                wire,
            });
        }

        let tag_u32 = tag as u32;
        if let Some((prev_tag, prev_wire)) = previous {
            if tag_u32 < prev_tag {
                return Err(Error::OutOfOrder {
                    offset: key_start,
                    previous: prev_tag,
                    tag: tag_u32,
                });
            }
            if tag_u32 == prev_tag && !(prev_wire == 2 && wire == 2) {
                return Err(Error::Duplicate {
                    offset: key_start,
                    tag: tag_u32,
                });
            }
        }

        match wire {
            0 => {
                let value = read_varint(input, &mut cursor, frame_end)?;
                if tag_u32 <= 15 {
                    message.fields.push(Field::UInt {
                        tag: tag_u32,
                        value,
                    });
                }
            }
            1 | 5 => {
                let size = if wire == 1 { 8usize } else { 4usize };
                let remaining_body = frame_end - cursor;
                if remaining_body < size {
                    return Err(Error::LengthOutOfBounds {
                        offset: key_start,
                        length: size as u64,
                    });
                }
                cursor += size;
            }
            2 => {
                let length_start = cursor;
                let length = read_varint(input, &mut cursor, frame_end)?;
                let remaining_body = frame_end - cursor;
                if length > remaining_body as u64 {
                    return Err(Error::LengthOutOfBounds {
                        offset: length_start,
                        length,
                    });
                }
                let data_len = usize::try_from(length).map_err(|_| {
                    Error::LengthOutOfBounds {
                        offset: length_start,
                        length,
                    }
                })?;
                let data_end = cursor + data_len;

                if tag_u32 <= 15 {
                    if let Some(Field::Bytes { tag: last_tag, data }) = message.fields.last_mut() {
                        if *last_tag == tag_u32 {
                            data.extend_from_slice(&input[cursor..data_end]);
                        } else {
                            message.fields.push(Field::Bytes {
                                tag: tag_u32,
                                data: input[cursor..data_end].to_vec(),
                            });
                        }
                    } else {
                        message.fields.push(Field::Bytes {
                            tag: tag_u32,
                            data: input[cursor..data_end].to_vec(),
                        });
                    }
                }
                cursor = data_end;
            }
            _ => unreachable!(),
        }

        previous = Some((tag_u32, wire));
    }

    Ok((message, frame_end))
}