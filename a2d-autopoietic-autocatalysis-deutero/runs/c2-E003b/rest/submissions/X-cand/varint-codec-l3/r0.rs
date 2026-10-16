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

fn write_varint(mut value: u64, output: &mut Vec<u8>) {
    while value >= 0x80 {
        output.push((value as u8 & 0x7f) | 0x80);
        value >>= 7;
    }
    output.push(value as u8);
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

        let wire = if is_bytes { 2u64 } else { 0u64 };
        write_varint(((tag as u64) << 3) | wire, &mut body);

        match field {
            Field::UInt { value, .. } => write_varint(*value, &mut body),
            Field::Bytes { data, .. } => {
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

    let header_start = start;
    let mut cursor = start;
    let declared_length = read_varint(input, &mut cursor, input.len())?;

    let remaining = input.len() - cursor;
    if declared_length > remaining as u64 {
        return Err(Error::LengthOutOfBounds {
            offset: header_start,
            length: declared_length,
        });
    }

    let body_length = declared_length as usize;
    let frame_end = cursor + body_length;
    let mut fields = Vec::new();
    let mut previous: Option<(u32, u8)> = None;

    while cursor < frame_end {
        let key_start = cursor;
        let key = read_varint(input, &mut cursor, frame_end)?;
        let tag64 = key >> 3;
        let wire = (key & 7) as u8;

        if !(1..=255).contains(&tag64) {
            return Err(Error::InvalidTag {
                offset: key_start,
                tag: tag64,
            });
        }
        let tag = tag64 as u32;

        if !matches!(wire, 0 | 1 | 2 | 5) {
            return Err(Error::InvalidWire {
                offset: key_start,
                wire,
            });
        }

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

        match wire {
            0 => {
                let value = read_varint(input, &mut cursor, frame_end)?;
                if tag <= 15 {
                    fields.push(Field::UInt { tag, value });
                }
            }
            1 | 5 => {
                let length = if wire == 1 { 8usize } else { 4usize };
                if length > frame_end - cursor {
                    return Err(Error::LengthOutOfBounds {
                        offset: key_start,
                        length: length as u64,
                    });
                }
                cursor += length;
            }
            2 => {
                let length_start = cursor;
                let length = read_varint(input, &mut cursor, frame_end)?;
                let remaining = frame_end - cursor;
                if length > remaining as u64 {
                    return Err(Error::LengthOutOfBounds {
                        offset: length_start,
                        length,
                    });
                }
                let data_length = length as usize;
                let data_end = cursor + data_length;

                if tag <= 15 {
                    if let Some(Field::Bytes {
                        tag: last_tag,
                        data,
                    }) = fields.last_mut()
                    {
                        if *last_tag == tag {
                            data.extend_from_slice(&input[cursor..data_end]);
                        } else {
                            fields.push(Field::Bytes {
                                tag,
                                data: input[cursor..data_end].to_vec(),
                            });
                        }
                    } else {
                        fields.push(Field::Bytes {
                            tag,
                            data: input[cursor..data_end].to_vec(),
                        });
                    }
                }

                cursor = data_end;
            }
            _ => unreachable!(),
        }

        previous = Some((tag, wire));
    }

    Ok((Message { fields }, frame_end))
}