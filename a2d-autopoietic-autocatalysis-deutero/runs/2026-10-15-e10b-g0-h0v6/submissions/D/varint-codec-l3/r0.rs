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

fn put_varint(mut value: u64, out: &mut Vec<u8>) {
    while value >= 0x80 {
        out.push((value as u8) | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

fn read_varint(
    input: &[u8],
    cursor: &mut usize,
    end: usize,
    start: usize,
) -> Result<u64, Error> {
    let mut value = 0u64;
    for i in 0..10 {
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

        if i == 9 && (payload > 1 || byte & 0x80 != 0) {
            return Err(Error::Overflow { offset, start });
        }

        value |= (payload as u64) << (7 * i);
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
        }

        previous = Some((tag, is_bytes));
    }

    let mut frame = Vec::new();
    put_varint(body.len() as u64, &mut frame);
    frame.extend_from_slice(&body);
    Ok(frame)
}

pub fn decode_message(input: &[u8], start: usize) -> Result<(Message, usize), Error> {
    if start > input.len() {
        return Err(Error::StartOutOfBounds { offset: start });
    }

    let mut cursor = start;
    let header_start = cursor;
    let declared_length = read_varint(input, &mut cursor, input.len(), header_start)?;

    let remaining = input.len() - cursor;
    if declared_length > remaining as u64 {
        return Err(Error::LengthOutOfBounds {
            offset: header_start,
            length: declared_length,
        });
    }

    let body_len = declared_length as usize;
    let frame_end = cursor + body_len;
    let mut fields = Vec::new();
    let mut previous: Option<(u32, u8)> = None;

    while cursor < frame_end {
        let key_start = cursor;
        let key = read_varint(input, &mut cursor, frame_end, key_start)?;
        let tag_value = key >> 3;
        let wire = (key & 7) as u8;

        if !(1..=255).contains(&tag_value) {
            return Err(Error::InvalidTag {
                offset: key_start,
                tag: tag_value,
            });
        }
        let tag = tag_value as u32;

        if wire != 0 && wire != 1 && wire != 2 && wire != 5 {
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
                let payload_start = cursor;
                let value = read_varint(input, &mut cursor, frame_end, payload_start)?;
                if tag <= 15 {
                    fields.push(Field::UInt { tag, value });
                }
            }
            1 | 5 => {
                let count = if wire == 1 { 8usize } else { 4usize };
                if frame_end - cursor < count {
                    return Err(Error::LengthOutOfBounds {
                        offset: key_start,
                        length: count as u64,
                    });
                }
                cursor += count;
            }
            2 => {
                let length_start = cursor;
                let length = read_varint(input, &mut cursor, frame_end, length_start)?;
                let remaining = frame_end - cursor;
                if length > remaining as u64 {
                    return Err(Error::LengthOutOfBounds {
                        offset: length_start,
                        length,
                    });
                }
                let data_len = length as usize;
                if tag <= 15 {
                    if let Some(Field::Bytes {
                        tag: previous_tag,
                        data,
                    }) = fields.last_mut()
                    {
                        if *previous_tag == tag {
                            data.extend_from_slice(&input[cursor..cursor + data_len]);
                        } else {
                            fields.push(Field::Bytes {
                                tag,
                                data: input[cursor..cursor + data_len].to_vec(),
                            });
                        }
                    } else {
                        fields.push(Field::Bytes {
                            tag,
                            data: input[cursor..cursor + data_len].to_vec(),
                        });
                    }
                }
                cursor += data_len;
            }
            _ => unreachable!(),
        }
    }

    Ok((Message { fields }, frame_end))
}